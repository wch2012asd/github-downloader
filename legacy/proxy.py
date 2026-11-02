"""Clash 代理检测与管理。"""

import socket
import time
import requests

try:
    import winreg
except ImportError:
    winreg = None


CLASH_COMMON_PORTS = [7890, 7897, 7891, 7892, 7899, 10809, 1080, 8080, 2080]
# 直接用 GitHub 官方端点验证连通性，对齐软件用途；httpbin.org 不稳定，常导致误报连接失败
TEST_URL = "https://api.github.com/zen"
TEST_TIMEOUT = 8


def check_port_open(port: int, host: str = "127.0.0.1") -> bool:
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.settimeout(1)
    try:
        result = sock.connect_ex((host, port))
        return result == 0
    except Exception:
        return False
    finally:
        sock.close()


def test_proxy(port: int, host: str = "127.0.0.1", scheme: str = "http") -> bool:
    """通过指定代理请求 GitHub，验证代理是否可用。"""
    proxy_url = f"{scheme}://{host}:{port}"
    proxies = {"http": proxy_url, "https": proxy_url}
    try:
        resp = requests.get(TEST_URL, proxies=proxies, timeout=TEST_TIMEOUT)
        return resp.status_code == 200
    except Exception:
        return False


def ping_proxy(port: int, host: str = "127.0.0.1", scheme: str = "http") -> int | None:
    """通过指定代理请求 GitHub，返回延迟毫秒；代理不可用返回 None。"""
    proxy_url = f"{scheme}://{host}:{port}"
    proxies = {"http": proxy_url, "https": proxy_url}
    start = time.perf_counter()
    try:
        resp = requests.get(TEST_URL, proxies=proxies, timeout=TEST_TIMEOUT)
    except Exception:
        return None
    if resp.status_code != 200:
        return None
    return round((time.perf_counter() - start) * 1000)


def get_system_proxy_port() -> int | None:
    """读取 Windows 系统代理端口（Clash 开启系统代理时写入注册表）；非 Windows 或未开启返回 None。"""
    if winreg is None:
        return None
    try:
        with winreg.OpenKey(
            winreg.HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\Internet Settings",
        ) as key:
            enabled = winreg.QueryValueEx(key, "ProxyEnable")[0]
            server = winreg.QueryValueEx(key, "ProxyServer")[0]
    except OSError:
        return None
    if not enabled or not isinstance(server, str):
        return None

    # ProxyServer 可能是 "127.0.0.1:7890"，也可能是 "http=...;https=...;socks=..." 形式
    ports: list[int] = []
    for part in server.split(";"):
        addr = part.split("=", 1)[-1].strip()
        _, _, port_str = addr.rpartition(":")
        if port_str.isdigit():
            ports.append(int(port_str))
    return ports[0] if ports else None


def detect_clash_proxy() -> int | None:
    # 系统代理端口最贴近用户实际配置，优先尝试
    sys_port = get_system_proxy_port()
    candidates = ([sys_port] if sys_port is not None else []) + CLASH_COMMON_PORTS
    tried: set[int] = set()
    for port in candidates:
        if port in tried:
            continue
        tried.add(port)
        if check_port_open(port) and test_proxy(port):
            return port
    return None


def get_proxy_dict(port: int, host: str = "127.0.0.1", scheme: str = "http") -> dict:
    proxy_url = f"{scheme}://{host}:{port}"
    return {"http": proxy_url, "https": proxy_url}
