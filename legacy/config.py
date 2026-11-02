"""配置持久化：将设置保存到程序同目录的 config.json。"""

import json
import os
import sys

DEFAULT_PROXY_PORT = 7890
DEFAULT_GHPROXY_PREFIX = "https://gh-proxy.com/"
DEFAULT_KKGITHUB_HOST = "https://kkgithub.com"


def _config_path() -> str:
    # PyInstaller 打包后 __file__ 指向临时解压目录，需取 EXE 所在目录
    if getattr(sys, "frozen", False):
        base_dir = os.path.dirname(sys.executable)
    else:
        base_dir = os.path.dirname(os.path.abspath(__file__))
    return os.path.join(base_dir, "config.json")


def load_config() -> dict:
    """读取配置文件；文件不存在或内容无效时返回空字典。"""
    try:
        with open(_config_path(), "r", encoding="utf-8") as f:
            data = json.load(f)
    except (OSError, ValueError):
        return {}
    return data if isinstance(data, dict) else {}


def save_config(
    proxy_port: int | None = None,
    save_dir: str | None = None,
    engine: str | None = None,
    auto_open: bool | None = None,
    history: list | None = None,
    ghproxy_prefix: str | None = None,
    kkgithub_host: str | None = None,
) -> bool:
    """合并保存配置，只更新传入的字段；返回是否保存成功。"""
    config = load_config()
    if proxy_port is not None:
        config["proxy_port"] = proxy_port
    if save_dir:
        config["save_dir"] = save_dir
    if engine is not None:
        config["engine"] = engine
    if auto_open is not None:
        config["auto_open"] = auto_open
    if history is not None:
        config["history"] = history
    if ghproxy_prefix:
        config["ghproxy_prefix"] = ghproxy_prefix
    if kkgithub_host:
        config["kkgithub_host"] = kkgithub_host
    try:
        with open(_config_path(), "w", encoding="utf-8") as f:
            json.dump(config, f, ensure_ascii=False, indent=2)
        return True
    except OSError:
        return False
