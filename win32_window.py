"""Win32 辅助：强制设置窗口/任务栏图标。"""

import ctypes
from ctypes import wintypes

WM_SETICON = 0x0080
ICON_SMALL = 0
ICON_BIG = 1
IMAGE_ICON = 1
LR_LOADFROMFILE = 0x10
GCLP_HICON = -14
GCLP_HICONSM = -34


def set_window_icon(hwnd: int, ico_path: str):
    """用 WM_SETICON + 类图标双通道强制设置窗口图标，确保任务栏刷新。"""
    user32 = ctypes.windll.user32
    hicon_big = user32.LoadImageW(None, ico_path, IMAGE_ICON, 32, 32, LR_LOADFROMFILE)
    hicon_small = user32.LoadImageW(None, ico_path, IMAGE_ICON, 16, 16, LR_LOADFROMFILE)
    if hicon_big:
        user32.SendMessageW(hwnd, WM_SETICON, ICON_BIG, hicon_big)
    if hicon_small:
        user32.SendMessageW(hwnd, WM_SETICON, ICON_SMALL, hicon_small)
    # 部分任务栏/预览路径读窗口类图标而非窗口消息图标
    set_class_long_ptr = getattr(user32, "SetClassLongPtrW", None)
    if set_class_long_ptr:
        if hicon_big:
            set_class_long_ptr(hwnd, GCLP_HICON, hicon_big)
        if hicon_small:
            set_class_long_ptr(hwnd, GCLP_HICONSM, hicon_small)
