"""下载通道引擎：为下载 URL 选择加速通道并测量通道延迟。"""

import time
import requests


TEST_URL = "https://api.github.com/zen"
PING_TIMEOUT = 5


class EngineChannel:
    def __init__(self, id: str, name: str, tag: str):
        self.id = id
        self.name = name
        self.tag = tag


CHANNELS = [
    EngineChannel("clash", "Clash 本地代理", "推荐"),
    EngineChannel("ghproxy", "GHProxy 镜像", "免客户端"),
    EngineChannel("kkgithub", "KKGitHub 节点", "极速"),
    EngineChannel("direct", "官方直连", "原始"),
]


def get_channel(engine_id: str) -> EngineChannel:
    for channel in CHANNELS:
        if channel.id == engine_id:
            return channel
    return CHANNELS[0]


def build_download_url(engine_id: str, url: str, ghproxy_prefix: str, kkgithub_host: str) -> str:
    """将原始 GitHub URL 转换为指定通道的下载地址。"""
    if engine_id == "ghproxy":
        return f"{ghproxy_prefix.rstrip('/')}/{url}"
    if engine_id == "kkgithub":
        kkgithub_host = kkgithub_host.rstrip("/")
        # kkgithub 镜像的 raw 域名为 raw.<主域名>
        raw_host = kkgithub_host.replace("://", "://raw.")
        url = url.replace("https://raw.githubusercontent.com", raw_host)
        url = url.replace("https://github.com", kkgithub_host)
        return url
    return url


def get_proxies(engine_id: str, clash_port: int | None) -> dict | None:
    """返回通道对应的 requests 代理配置；镜像/直连通道不走本地代理。"""
    if engine_id == "clash" and clash_port:
        proxy_url = f"http://127.0.0.1:{clash_port}"
        return {"http": proxy_url, "https": proxy_url}
    return None


def measure_latency(engine_id: str, clash_port: int | None, ghproxy_prefix: str, kkgithub_host: str) -> int | None:
    """测量通道连通延迟（毫秒）；通道不可用返回 None。"""
    if engine_id == "clash":
        proxies = get_proxies("clash", clash_port)
        if not proxies:
            return None
        url = TEST_URL
    elif engine_id == "ghproxy":
        url, proxies = ghproxy_prefix, None
    elif engine_id == "kkgithub":
        url, proxies = kkgithub_host, None
    else:
        url, proxies = TEST_URL, None

    start = time.perf_counter()
    try:
        resp = requests.get(url, proxies=proxies, timeout=PING_TIMEOUT)
    except requests.RequestException:
        return None
    if engine_id in ("clash", "direct") and resp.status_code != 200:
        return None
    return round((time.perf_counter() - start) * 1000)
