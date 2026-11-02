"""界面图标：PIL 绘制矢量线条图标 + GitHub 官方标志，输出 CTkImage。

图标在 24x24 网格上以 4 倍超采样绘制后缩小，保证边缘平滑。
"""

import os
import sys

from PIL import Image, ImageDraw

_SS = 4          # 超采样倍数
_GRID = 24       # 设计网格
_CACHE: dict = {}


def asset_path(name: str) -> str:
    """资源文件路径；PyInstaller 打包后位于 _MEIPASS 解压目录。"""
    base = getattr(sys, "_MEIPASS", os.path.dirname(os.path.abspath(__file__)))
    return os.path.join(base, "assets", name)


def _capsule(draw: ImageDraw.ImageDraw, points, width, color):
    """圆头折线（端点补圆）。坐标已按超采样放大。"""
    draw.line(points, fill=color, width=width, joint="curve")
    r = width / 2
    for p in (points[0], points[-1]):
        draw.ellipse([p[0] - r, p[1] - r, p[0] + r, p[1] + r], fill=color)


def _icon_specs(name: str):
    """返回图标的绘制指令列表，坐标基于 24 网格。"""
    specs = {
        # 链条：两个对角相扣的马蹄形
        "link": [
            ("arc", (3, 10, 14, 21), 345, 285, 2),
            ("arc", (10, 3, 21, 14), 165, 105, 2),
        ],
        # 文件夹
        "folder": [
            ("line", [(3, 20), (3, 7), (9, 7), (11, 9.5), (21, 9.5), (21, 20), (3, 20)], 2),
        ],
        # 信号（Wi-Fi 弧线 + 圆点）
        "signal": [
            ("dot", (12, 17.5), 1.8),
            ("arc", (7, 13, 17, 23), 225, 315, 2),
            ("arc", (3, 9, 21, 27), 225, 315, 2),
        ],
        # 环形箭头（下载进行中）：300° 圆弧 + 顶端箭头
        "gauge": [
            ("arc", (4, 4, 20, 20), 300, 240, 2),
            ("poly", [(11.9, 2.8), (8.2, 8.4), (5.2, 3.2)]),
        ],
        # 终端（方框 + >_）
        "terminal": [
            ("rrect", (3, 4, 21, 20), 2.5, 2),
            ("line", [(7, 9), (11, 13), (7, 17)], 2),
            ("line", [(13, 17), (17, 17)], 2),
        ],
        # 剪贴板（粘贴）
        "clipboard": [
            ("rrect", (6, 5, 18, 21), 2.5, 2),
            ("rrect", (9, 2.8, 15, 7), 1.5, None),
            ("line", [(9, 12), (15, 12)], 2),
            ("line", [(9, 16), (13, 16)], 2),
        ],
        # 时钟（任务记录）
        "clock": [
            ("dot", (12, 12), 8, 2),
            ("line", [(12, 7.5), (12, 12.5), (16, 14.5)], 2),
        ],
    }
    return specs.get(name, [])


def render(name: str, color: str, size: int = 18) -> Image.Image:
    """绘制图标，返回高分辨率 RGBA 图像（显示尺寸的 2 倍，供缩放保持清晰）。"""
    key = (name, color, size)
    if key in _CACHE:
        return _CACHE[key]

    img = Image.new("RGBA", (_GRID * _SS, _GRID * _SS), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    s = _SS

    for spec in _icon_specs(name):
        kind = spec[0]
        if kind == "line":
            _, points, width = spec
            _capsule(draw, [(x * s, y * s) for x, y in points], width * s, color)
        elif kind == "arc":
            _, bbox, start, end, width = spec
            draw.arc([v * s for v in bbox], start, end, fill=color, width=width * s)
        elif kind == "dot":
            _, center, radius, *rest = spec
            box = [((center[0] - radius) * s), ((center[1] - radius) * s),
                   ((center[0] + radius) * s), ((center[1] + radius) * s)]
            if rest:  # 描边圆
                draw.ellipse(box, outline=color, width=rest[0] * s)
            else:     # 实心圆
                draw.ellipse(box, fill=color)
        elif kind == "poly":
            draw.polygon([(x * s, y * s) for x, y in spec[1]], fill=color)
        elif kind == "rrect":
            _, bbox, radius, width = spec
            box = [v * s for v in bbox]
            if width:  # 描边圆角矩形
                draw.rounded_rectangle(box, radius=radius * s, outline=color, width=width * s)
            else:      # 实心圆角矩形
                draw.rounded_rectangle(box, radius=radius * s, fill=color)

    img = img.resize((size * 2, size * 2), Image.LANCZOS)
    _CACHE[key] = img
    return img


def ctk_image(name: str, color: str, size: int = 18):
    import customtkinter as ctk
    key = (name, color, size)
    if key in _CACHE:
        return _CACHE[key]
    # assets/<name>.png 存在时优先使用外部图标（按 alpha 遮罩着色）
    asset_file = asset_path(f"{name}.png")
    if os.path.exists(asset_file):
        base = Image.open(asset_file).convert("RGBA")
        solid = Image.new("RGBA", base.size, color)
        solid.putalpha(base.split()[3])
        image = ctk.CTkImage(light_image=solid, dark_image=solid, size=(size, size))
        _CACHE[key] = image
        return image
    image = ctk.CTkImage(light_image=render(name, color, size),
                         dark_image=render(name, color, size), size=(size, size))
    _CACHE[key] = image
    return image
