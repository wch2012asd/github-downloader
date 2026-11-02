"""下载器核心：断点续传、自动重试、进度回调、速度计算。"""

import os
import time
import threading
import requests
from dataclasses import dataclass
from typing import Callable


@dataclass
class DownloadProgress:
    downloaded: int = 0
    total: int = 0
    speed: float = 0.0
    percentage: float = 0.0


class Downloader:
    CHUNK_SIZE = 8192
    MAX_ATTEMPTS = 8        # 首次下载 + 最多 7 次自动重试
    RETRY_DELAY = 5         # 每次重试前的等待秒数
    USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) GitHubDownloader/1.0"

    def __init__(
        self,
        proxy_dict: dict | None = None,
        progress_callback: Callable[[DownloadProgress], None] | None = None,
        log_callback: Callable[[str, str], None] | None = None,
    ):
        self.proxy_dict = proxy_dict
        self.progress_callback = progress_callback
        self.log_callback = log_callback
        self._stop = False
        self._pause_event = threading.Event()
        self._pause_event.set()

    def _log(self, msg: str, level: str = "INFO"):
        if self.log_callback:
            self.log_callback(msg, level)

    def stop(self):
        self._stop = True
        self._pause_event.set()

    def pause(self):
        self._pause_event.clear()
        self._log("下载已暂停")

    def resume(self):
        self._pause_event.set()
        self._log("下载继续")

    def download(self, url: str, save_path: str, headers: dict | None = None) -> bool:
        """下载文件；单次尝试失败后自动从已下载位置续传重试，返回是否成功。"""
        self._stop = False
        self._pause_event.set()

        for attempt in range(1, self.MAX_ATTEMPTS + 1):
            ok, retryable = self._download_attempt(url, save_path, headers)
            if ok or self._stop or not retryable or attempt >= self.MAX_ATTEMPTS:
                return ok
            self._log(f"{self.RETRY_DELAY} 秒后自动重试（第 {attempt}/{self.MAX_ATTEMPTS - 1} 次）...", "WARN")
            # 等待期间保持取消按钮可用
            for _ in range(self.RETRY_DELAY * 10):
                if self._stop:
                    return False
                time.sleep(0.1)
        return False

    def _download_attempt(self, url: str, save_path: str, headers: dict | None) -> tuple[bool, bool]:
        """单次下载尝试；返回 (是否成功, 失败是否值得重试)。"""
        final_headers = {"User-Agent": self.USER_AGENT}
        if headers:
            final_headers.update(headers)

        # 每次尝试都按当前文件大小续传，重试才能接上上次中断的位置
        existing_size = 0
        if os.path.exists(save_path):
            existing_size = os.path.getsize(save_path)
            if existing_size > 0:
                final_headers["Range"] = f"bytes={existing_size}-"
                self._log(f"检测到已有文件，从 {existing_size} 字节处断点续传")

        try:
            resp = requests.get(
                url,
                headers=final_headers,
                proxies=self.proxy_dict,
                stream=True,
                timeout=30,
                allow_redirects=True,
            )
            resp.raise_for_status()
        except requests.HTTPError as e:
            return self._on_http_error(e, url, existing_size)
        except requests.RequestException as e:
            self._log(f"连接失败: {e}", "ERROR")
            return False, True

        total_header = resp.headers.get("Content-Length")
        if existing_size > 0 and resp.status_code == 206:
            total = int(total_header) + existing_size if total_header else 0
        elif resp.status_code == 200 and existing_size > 0:
            existing_size = 0
            total = int(total_header) if total_header else 0
            self._log("服务器不支持断点续传，重新下载", "WARN")
        elif resp.status_code == 200:
            total = int(total_header) if total_header else 0
        else:
            # raise_for_status 已拦截 4xx/5xx，正常不会走到这里
            self._log(f"服务器返回异常状态码: {resp.status_code}", "ERROR")
            return False, True

        mode = "ab" if existing_size > 0 else "wb"
        downloaded = existing_size
        last_update = time.time()
        last_bytes = downloaded

        try:
            with open(save_path, mode) as f:
                for chunk in resp.iter_content(self.CHUNK_SIZE):
                    if self._stop:
                        self._log("下载已取消", "WARN")
                        return False, False

                    self._pause_event.wait()

                    if chunk:
                        f.write(chunk)
                        downloaded += len(chunk)

                        now = time.time()
                        if now - last_update >= 0.5:
                            elapsed = now - last_update
                            speed = (downloaded - last_bytes) / elapsed if elapsed > 0 else 0
                            percentage = (downloaded / total * 100) if total > 0 else 0
                            if self.progress_callback:
                                self.progress_callback(DownloadProgress(
                                    downloaded=downloaded,
                                    total=total,
                                    speed=speed,
                                    percentage=percentage,
                                ))
                            last_update = now
                            last_bytes = downloaded

                # 最终进度
                percentage = 100.0 if total == 0 or downloaded >= total else (downloaded / total * 100)
                if self.progress_callback:
                    self.progress_callback(DownloadProgress(
                        downloaded=downloaded,
                        total=total if total > 0 else downloaded,
                        speed=0,
                        percentage=percentage,
                    ))

            self._log(f"下载完成: {save_path} ({downloaded} 字节)", "SUCCESS")
            return True, False

        except requests.RequestException as e:
            # 连接中断（超时/重置等）：已写入的部分保留，交给外层续传重试
            self._log(f"下载中断: {e}", "WARN")
            return False, True
        except Exception as e:
            self._log(f"下载出错: {e}", "ERROR")
            return False, False

    def _on_http_error(self, e: requests.HTTPError, url: str, existing_size: int) -> tuple[bool, bool]:
        status = e.response.status_code if e.response is not None else 0
        if status == 416 and existing_size > 0:
            # Range 起点越界通常是文件已完整，用 HEAD 比对大小确认
            remote_size = self._remote_size(url)
            if remote_size is not None and existing_size >= remote_size:
                self._log(f"文件已完整，无需下载 ({existing_size} 字节)", "SUCCESS")
                if self.progress_callback:
                    self.progress_callback(DownloadProgress(
                        downloaded=existing_size, total=existing_size, speed=0, percentage=100.0,
                    ))
                return True, False
            self._log(f"服务器拒绝续传 (416)，本地文件 ({existing_size} 字节) 与服务器不一致，请删除后重试", "ERROR")
            return False, False
        self._log(f"连接失败: HTTP {status}", "ERROR")
        return False, 500 <= status < 600

    def _remote_size(self, url: str) -> int | None:
        try:
            resp = requests.head(
                url,
                headers={"User-Agent": self.USER_AGENT},
                proxies=self.proxy_dict,
                timeout=30,
                allow_redirects=True,
            )
            if resp.ok:
                length = resp.headers.get("Content-Length")
                return int(length) if length else None
        except (requests.RequestException, ValueError):
            pass
        return None
