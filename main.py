"""GitHub 加速下载器 - 通过 Clash 代理或镜像通道加速 GitHub 文件下载。"""

import os
import time
import shutil
import ctypes
import threading
import tkinter as tk
import requests
import icons
import customtkinter as ctk
from tkinter import filedialog, messagebox

import win32_window
from url_parser import parse_github_url, GitHubLinkType
from proxy import detect_clash_proxy, ping_proxy, check_port_open
from downloader import Downloader, DownloadProgress
from engines import CHANNELS, build_download_url, get_proxies, measure_latency, get_channel
from config import load_config, save_config, DEFAULT_PROXY_PORT, DEFAULT_GHPROXY_PREFIX, DEFAULT_KKGITHUB_HOST

# ── 配色 ──
C_BG = "#f3f4f6"
C_CARD = "#ffffff"
C_BORDER = "#e5e7eb"
C_TEXT = "#1f2937"
C_MUTED = "#9ca3af"
C_ACCENT = "#2563eb"
C_ACCENT_HOVER = "#1d4ed8"
C_ACCENT_SOFT = "#eff6ff"
C_GREEN = "#16a34a"
C_GREEN_SOFT = "#f0fdf4"
C_GREEN_HOVER = "#dcfce7"
C_RED = "#dc2626"
C_BTN = "#f3f4f6"
C_BTN_HOVER = "#e2e5ea"
C_INPUT_BORDER = "#cbd5e1"   # 输入框描边比卡片边框深，避免发虚
C_LOG_BG = "#0d1117"
C_LOG_FG = "#c9d1d9"
C_LOG_MUTED = "#6e7681"
LOG_LEVEL_COLORS = {"INFO": "#58a6ff", "WARN": "#e3b341", "SUCCESS": "#3fb950", "ERROR": "#f85149"}

TYPE_META = {
    GitHubLinkType.RAW_FILE: "Raw 文件",
    GitHubLinkType.RELEASE_ASSET: "Release 资产包",
    GitHubLinkType.REPO_ARCHIVE: "仓库压缩包",
    GitHubLinkType.REPO_PAGE: "仓库源码包 (main.zip)",
    GitHubLinkType.UNKNOWN: "未知类型",
}


class GitHubDownloaderApp:
    WINDOW_TITLE = "GitHub 加速下载器"
    VERSION = "v1.0"
    PROXY_CHECK_INTERVAL_MS = 30_000
    THREAD_LABEL = "1T"     # 当前为单线程下载引擎
    PRESETS = [
        ("LumaShot", "https://github.com/jimmgreen/LumaShot"),
        ("Clash", "https://github.com/clash-verge-rev/clash-verge-rev"),
        ("VS", "https://github.com/microsoft/vscode"),
    ]

    def __init__(self, root: ctk.CTk):
        self.root = root
        root.title(f"{self.WINDOW_TITLE} {self.VERSION}")
        root.geometry("1280x820")
        root.minsize(1120, 720)
        root.configure(fg_color=C_BG)
        try:
            root.iconbitmap(icons.asset_path("app_icon.ico"))
        except tk.TclError:
            pass

        config = load_config()
        save_dir = config.get("save_dir")
        if not isinstance(save_dir, str) or not save_dir:
            save_dir = os.getcwd()
        try:
            proxy_port = int(config.get("proxy_port", DEFAULT_PROXY_PORT))
        except (TypeError, ValueError):
            proxy_port = DEFAULT_PROXY_PORT

        self.engine_id = config.get("engine", "clash")
        if self.engine_id not in {c.id for c in CHANNELS}:
            self.engine_id = "clash"
        self.ghproxy_prefix = config.get("ghproxy_prefix") or DEFAULT_GHPROXY_PREFIX
        self.kkgithub_host = config.get("kkgithub_host") or DEFAULT_KKGITHUB_HOST
        self.history = config.get("history") if isinstance(config.get("history"), list) else []

        # ── 字体（需在 Tk 根窗口创建后初始化）──
        self.f_title = ctk.CTkFont(family="Microsoft YaHei UI", size=17, weight="bold")
        self.f_base = ctk.CTkFont(family="Microsoft YaHei UI", size=13)
        self.f_base_b = ctk.CTkFont(family="Microsoft YaHei UI", size=13, weight="bold")
        self.f_small = ctk.CTkFont(family="Microsoft YaHei UI", size=11)
        self.f_mono = ctk.CTkFont(family="Consolas", size=13)
        self.f_log = ctk.CTkFont(family="Consolas", size=11)

        # ── 运行状态 ──
        self.downloader: Downloader | None = None
        self.proxy_port: int | None = None
        self.proxy_dict: dict | None = None
        self.proxy_latency: int | None = None
        self._proxy_online = False
        self._proxy_check_ran = False
        self._check_gen = 0
        self._url_parse_job: str | None = None
        self._size_job_gen = 0
        self.parsed = None
        self.estimated_size: int | None = None
        self.channel_latency: dict[str, int | None] = {}
        self.running = False
        self.paused = False
        self.last_downloaded = 0
        self._log_records: list[tuple[str, str, str]] = []
        self.autoscroll = True

        # ── 界面变量 ──
        self.url_var = tk.StringVar()
        self.port_var = tk.StringVar(value=str(proxy_port))
        self.save_dir_var = tk.StringVar(value=save_dir)
        self.filename_var = tk.StringVar()
        self.auto_open_var = tk.BooleanVar(value=bool(config.get("auto_open", False)))
        self.link_type_var = tk.StringVar(value="未解析")
        self.size_var = tk.StringVar(value="预估大小: --")
        self.net_status_var = tk.StringVar(value="检测中...")
        self.progress_status_var = tk.StringVar(value="就绪")
        self.pct_var = tk.StringVar(value="0.0%")
        self.speed_var = tk.StringVar(value="--")
        self.transferred_var = tk.StringVar(value="-- / --")
        self.eta_var = tk.StringVar(value=f"-- · {self.THREAD_LABEL}")
        self.disk_free_var = tk.StringVar(value="可用: --")
        self.path_preview_var = tk.StringVar()
        self.ext_var = tk.StringVar(value="")
        self.log_search_var = tk.StringVar()
        self.log_level_var = tk.StringVar(value="全部")
        self.log_count_var = tk.StringVar(value="运行日志 (0)")

        self._build_ui()

        self.url_var.trace_add("write", self._on_url_changed)
        self.filename_var.trace_add("write", lambda *_: self._update_save_hints())
        self.save_dir_var.trace_add("write", lambda *_: self._update_save_hints())
        self.log_search_var.trace_add("write", lambda *_: self._render_log())
        self.log_level_var.trace_add("write", lambda *_: self._render_log())

        # 启动后台检测：代理状态 + 各通道延迟 + 磁盘余量
        self._update_save_hints()
        self._apply_window_icon()
        self.root.after(800, self._run_proxy_check)
        self.root.after(1000, self._measure_channels_async)

    def _apply_window_icon(self):
        """设置窗口/任务栏图标。"""
        try:
            self.root.update_idletasks()
            hwnd = ctypes.windll.user32.GetParent(self.root.winfo_id())
        except (OSError, AttributeError):
            return
        if hwnd:
            win32_window.set_window_icon(hwnd, icons.asset_path("app_icon.ico"))

    # ════════════════ 通用控件 ════════════════

    def _make_button(self, parent, text, command, kind="normal", width=None, height=30, icon=None, icon_color=None):
        palette = {
            "primary": (C_ACCENT, C_ACCENT_HOVER, "white"),
            "normal": (C_BTN, C_BTN_HOVER, C_TEXT),
            "green": (C_GREEN_SOFT, C_GREEN_HOVER, C_GREEN),
            "ghost": ("transparent", C_BTN, C_MUTED),
        }[kind]
        kwargs = {}
        if width is not None:
            kwargs["width"] = width
        if icon:
            kwargs["image"] = icons.ctk_image(icon, icon_color or palette[2], 15)
            kwargs["compound"] = "left"
            kwargs["anchor"] = "center"
        return ctk.CTkButton(parent, text=text, command=command, font=self.f_base,
                             fg_color=palette[0], hover_color=palette[1], text_color=palette[2],
                             corner_radius=8, height=height, **kwargs)

    def _chip(self, parent, text, command):
        # CTk 6.0 需要显式宽度，按文字长度估算（CJK 算全宽）
        width = 16 + sum(14 if ord(ch) > 0x2E7F else 8 for ch in text)
        return ctk.CTkButton(parent, text=text, command=command, font=self.f_small,
                             fg_color=C_BTN, hover_color=C_BTN_HOVER, text_color=C_MUTED,
                             corner_radius=6, width=width, height=22)

    def _card(self, parent, fill="x", expand=False):
        card = ctk.CTkFrame(parent, corner_radius=14, fg_color=C_CARD,
                            border_width=1, border_color=C_BORDER)
        card.pack(fill=fill, expand=expand, pady=(0, 10))
        return card

    def _card_header(self, card, title, icon=None, icon_color=None):
        bar = ctk.CTkFrame(card, fg_color="transparent")
        bar.pack(fill="x", padx=14, pady=(12, 4))
        if icon:
            ctk.CTkLabel(bar, text="", image=icons.ctk_image(icon, icon_color or C_ACCENT, 17)).pack(side="left")
            pad = (7, 0)
        else:
            pad = None
        if title:
            ctk.CTkLabel(bar, text=title, font=self.f_base_b, text_color=C_TEXT).pack(side="left", padx=pad)
        return bar

    # ════════════════ 界面搭建 ════════════════

    def _build_ui(self):
        container = ctk.CTkFrame(self.root, fg_color="transparent")
        container.pack(fill="both", expand=True)
        container.grid_rowconfigure(0, weight=1)
        container.grid_columnconfigure(0, weight=1, uniform="cols")
        container.grid_columnconfigure(1, weight=1, uniform="cols")

        left = ctk.CTkFrame(container, fg_color="transparent")
        left.grid(row=0, column=0, sticky="nsew", padx=(12, 4), pady=(12, 4))
        right = ctk.CTkFrame(container, fg_color="transparent")
        right.grid(row=0, column=1, sticky="nsew", padx=(4, 12), pady=(12, 4))
        left.grid_rowconfigure(2, weight=1)
        right.grid_rowconfigure(1, weight=1)

        self._build_link_card(left)
        self._build_save_card(left)
        self._build_network_card(left)
        self._build_progress_card(right)
        self._build_log_card(right)

    # ── 左栏：GitHub 链接 ──

    def _build_link_card(self, parent):
        card = self._card(parent)

        head = self._card_header(card, "GitHub 链接", icon="link")
        ctk.CTkLabel(head, text="自动识别 Release/源码/Raw", font=self.f_small, text_color=C_MUTED).pack(side="left", padx=(8, 0))
        preset_box = ctk.CTkFrame(head, fg_color="transparent")
        preset_box.pack(side="right")
        ctk.CTkLabel(preset_box, text="预设:", font=self.f_small, text_color=C_MUTED).pack(side="left", padx=(0, 6))
        for name, url in self.PRESETS:
            self._chip(preset_box, name, lambda u=url: self._apply_preset(u)).pack(side="left", padx=(0, 4))

        row = ctk.CTkFrame(card, fg_color="transparent")
        row.pack(fill="x", padx=14, pady=(6, 4))
        entry_holder = ctk.CTkFrame(row, fg_color="transparent")
        entry_holder.pack(side="left", fill="x", expand=True)
        self.url_entry = ctk.CTkEntry(entry_holder, textvariable=self.url_var, font=self.f_mono,
                                      fg_color="white", border_color=C_INPUT_BORDER, border_width=1,
                                      corner_radius=8, height=38)
        self.url_entry.pack(side="left", fill="x", expand=True)
        self.url_entry.bind("<Return>", lambda e: self._parse_url())
        clear_btn = ctk.CTkButton(entry_holder, text="×", font=self.f_base, width=26, height=24,
                                  fg_color="transparent", hover_color=C_BTN, text_color=C_MUTED,
                                  corner_radius=6, command=self._clear_url)
        clear_btn.place(relx=1.0, rely=0.5, x=-32, anchor="e")
        self._make_button(row, "粘贴", self._paste_url, icon="clipboard").pack(side="left", padx=(8, 6))
        self._make_button(row, "解析 →", self._parse_url, kind="primary").pack(side="left")

        info = ctk.CTkFrame(card, fg_color="transparent")
        info.pack(fill="x", padx=14, pady=(2, 12))
        type_box = ctk.CTkFrame(info, fg_color="transparent")
        type_box.pack(side="left")
        ctk.CTkLabel(type_box, text="资源类型:", font=self.f_base, text_color=C_MUTED).pack(side="left")
        ctk.CTkLabel(type_box, textvariable=self.link_type_var, font=self.f_base_b, text_color=C_ACCENT).pack(side="left", padx=(6, 0))
        ctk.CTkLabel(info, textvariable=self.size_var, font=self.f_base, text_color=C_MUTED).pack(side="right")

    # ── 左栏：保存位置与文件名 ──

    def _build_save_card(self, parent):
        card = self._card(parent)
        head = self._card_header(card, "保存位置与文件名", icon="folder", icon_color="#f59e0b")
        ctk.CTkLabel(head, textvariable=self.disk_free_var, font=self.f_base, text_color=C_MUTED).pack(side="right")

        row0 = ctk.CTkFrame(card, fg_color="transparent")
        row0.pack(fill="x", padx=14, pady=(4, 4))
        ctk.CTkLabel(row0, text="存储目录:", font=self.f_base, text_color=C_MUTED).pack(side="left")
        quick = ctk.CTkFrame(row0, fg_color="transparent")
        quick.pack(side="right")
        ctk.CTkLabel(quick, text="快捷:", font=self.f_small, text_color=C_MUTED).pack(side="left", padx=(0, 6))
        for label, path in self._quick_dirs():
            self._chip(quick, label, lambda p=path: self._set_quick_dir(p)).pack(side="left", padx=(0, 4))

        row1 = ctk.CTkFrame(card, fg_color="transparent")
        row1.pack(fill="x", padx=14, pady=(0, 6))
        self._make_button(row1, "选择", self._choose_dir).pack(side="left", padx=(0, 8))
        ctk.CTkEntry(row1, textvariable=self.save_dir_var, font=self.f_base, fg_color="white",
                     border_color=C_INPUT_BORDER, border_width=1, corner_radius=8, height=36).pack(side="left", fill="x", expand=True)

        row2 = ctk.CTkFrame(card, fg_color="transparent")
        row2.pack(fill="x", padx=14, pady=(0, 8))
        name_holder = ctk.CTkFrame(row2, fg_color="transparent")
        name_holder.pack(side="left", fill="x", expand=True)
        ctk.CTkEntry(name_holder, textvariable=self.filename_var, font=self.f_mono, fg_color="white",
                     border_color=C_INPUT_BORDER, border_width=1, corner_radius=8, height=36).pack(side="left", fill="x", expand=True)
        ctk.CTkLabel(name_holder, textvariable=self.ext_var, font=self.f_small,
                     fg_color="#eef0f3", text_color=C_MUTED, corner_radius=5).place(relx=1.0, rely=0.5, x=-10, anchor="e")

        row3 = ctk.CTkFrame(card, fg_color="transparent")
        row3.pack(fill="x", padx=14, pady=(0, 12))
        ctk.CTkCheckBox(row3, text="下载完成后自动打开文件目录", variable=self.auto_open_var,
                        command=self._on_toggle_auto_open, font=self.f_base, corner_radius=5,
                        fg_color=C_ACCENT, border_color="#cbd2dc").pack(side="left")
        ctk.CTkLabel(row3, textvariable=self.path_preview_var, font=self.f_small, text_color=C_MUTED).pack(side="right")

    # ── 左栏：网络加速与 Clash 代理 ──

    def _build_network_card(self, parent):
        card = self._card(parent)
        head = self._card_header(card, "网络加速与 Clash 代理", icon="signal", icon_color="#8b5cf6")
        self.net_status_label = ctk.CTkLabel(head, textvariable=self.net_status_var, font=self.f_base, text_color=C_MUTED)
        self.net_status_label.pack(side="right")

        row = ctk.CTkFrame(card, fg_color="transparent")
        row.pack(fill="x", padx=14, pady=(4, 6))
        ctk.CTkLabel(row, text="端口:", font=self.f_base, text_color=C_MUTED).pack(side="left")
        ctk.CTkEntry(row, textvariable=self.port_var, font=self.f_mono, width=90, fg_color="white",
                     border_color=C_INPUT_BORDER, border_width=1, corner_radius=8, height=34).pack(side="left", padx=(6, 8))
        self._make_button(row, "自动检测", self._auto_detect_proxy).pack(side="left", padx=(0, 6))
        self._make_button(row, "测试连接", self._test_proxy_connection).pack(side="left")
        quick = ctk.CTkFrame(row, fg_color="transparent")
        quick.pack(side="right")
        ctk.CTkLabel(quick, text="快捷:", font=self.f_small, text_color=C_MUTED).pack(side="left", padx=(0, 6))
        for port in (7890, 7891, 10808):
            self._chip(quick, str(port), lambda p=port: self._set_quick_port(p)).pack(side="left", padx=(0, 4))

        sub = ctk.CTkFrame(card, fg_color="transparent")
        sub.pack(fill="x", padx=14, pady=(4, 0))
        ctk.CTkLabel(sub, text="加速引擎通道", font=self.f_base_b, text_color=C_TEXT).pack(side="left")
        ctk.CTkLabel(sub, text="点击切换", font=self.f_small, text_color=C_MUTED).pack(side="right")

        grid = ctk.CTkFrame(card, fg_color="transparent")
        grid.pack(fill="x", padx=14, pady=(6, 12))
        self.channel_frames: dict[str, ctk.CTkFrame] = {}
        self.channel_latency_vars: dict[str, tk.StringVar] = {}
        for index, channel in enumerate(CHANNELS):
            cell = ctk.CTkFrame(grid, corner_radius=10, fg_color=C_CARD, border_width=1, border_color=C_BORDER)
            cell.grid(row=0, column=index, sticky="nsew", padx=(0, 8) if index < len(CHANNELS) - 1 else 0)
            grid.grid_columnconfigure(index, weight=1, uniform="engines")

            inner = ctk.CTkFrame(cell, fg_color="transparent")
            inner.pack(fill="both", expand=True, padx=10, pady=8)
            ctk.CTkLabel(inner, text=channel.name, font=self.f_base_b, text_color=C_TEXT, anchor="w").pack(fill="x")
            lat_row = ctk.CTkFrame(inner, fg_color="transparent")
            lat_row.pack(fill="x", pady=(2, 0))
            lat_var = tk.StringVar(value="未测速")
            ctk.CTkLabel(lat_row, textvariable=lat_var, font=self.f_small, text_color=C_MUTED).pack(side="left")
            ctk.CTkLabel(lat_row, text=channel.tag, font=self.f_small,
                         fg_color="#eef0f3", text_color=C_MUTED, corner_radius=5).pack(side="right")

            cell.bind("<Button-1>", lambda e, cid=channel.id: self._select_engine(cid))
            for widget in inner.winfo_children():
                for w in ([widget] + widget.winfo_children()):
                    w.bind("<Button-1>", lambda e, cid=channel.id: self._select_engine(cid))
            self.channel_frames[channel.id] = cell
            self.channel_latency_vars[channel.id] = lat_var

        self._apply_engine_visual()

    # ── 右栏：下载进度 ──

    def _build_progress_card(self, parent):
        card = self._card(parent)
        head = self._card_header(card, "下载进度与传输状态", icon="gauge", icon_color=C_GREEN)
        self.progress_status_label = ctk.CTkLabel(head, textvariable=self.progress_status_var, font=self.f_base, text_color=C_MUTED)
        self.progress_status_label.pack(side="right")

        self.progress_bar = ctk.CTkProgressBar(card, progress_color=C_ACCENT, fg_color=C_BORDER,
                                               corner_radius=4, height=8)
        self.progress_bar.set(0)
        self.progress_bar.pack(fill="x", padx=14, pady=(4, 10))

        stats = ctk.CTkFrame(card, fg_color="transparent")
        stats.pack(fill="x", padx=14, pady=(0, 10))
        stats.grid_columnconfigure((0, 1, 2, 3), weight=1, uniform="stats")
        for index, (caption, var) in enumerate([
            ("完成度", self.pct_var), ("当前速度", self.speed_var),
            ("已传输 / 总大小", self.transferred_var), ("剩余时间 · 线程", self.eta_var),
        ]):
            cell = ctk.CTkFrame(stats, fg_color="transparent")
            cell.grid(row=0, column=index, sticky="w")
            ctk.CTkLabel(cell, text=caption, font=self.f_small, text_color=C_MUTED, anchor="w").pack(fill="x")
            ctk.CTkLabel(cell, textvariable=var, font=self.f_base_b, text_color=C_TEXT, anchor="w").pack(fill="x", pady=(2, 0))

        buttons = ctk.CTkFrame(card, fg_color="transparent")
        buttons.pack(fill="x", padx=14, pady=(0, 12))
        self.primary_btn = self._make_button(buttons, "▶ 开始下载", self._on_primary_click, kind="primary", width=140)
        self.primary_btn.pack(side="left")
        self.cancel_btn = self._make_button(buttons, "取消", self._cancel_download, width=100)
        self.cancel_btn.pack(side="left", padx=(8, 0))
        self.cancel_btn.configure(state="disabled")
        self._make_button(buttons, "打开目录", self._open_dir, icon="folder", icon_color=C_TEXT, width=124).pack(side="right")
        self._make_button(buttons, "任务记录", self._show_history, icon="clock", width=124).pack(side="right", padx=(0, 8))

    # ── 右栏：运行日志 ──

    def _build_log_card(self, parent):
        card = self._card(parent, fill="both", expand=True)

        head = self._card_header(card, "", icon="terminal", icon_color=C_TEXT)
        ctk.CTkLabel(head, textvariable=self.log_count_var, font=self.f_base_b, text_color=C_TEXT).pack(side="left")
        tools = ctk.CTkFrame(head, fg_color="transparent")
        tools.pack(side="right")

        self.autoscroll_chip = self._chip(tools, "滚屏", self._toggle_autoscroll)
        self.autoscroll_chip.pack(side="right", padx=(4, 0))
        self._chip(tools, "导出", self._export_log).pack(side="right", padx=(4, 0))
        self._chip(tools, "复制", self._copy_log).pack(side="right", padx=(4, 0))
        self._chip(tools, "清空", self._clear_log).pack(side="right", padx=(4, 0))
        ctk.CTkOptionMenu(tools, values=["全部", "INFO", "WARN", "SUCCESS", "ERROR"], variable=self.log_level_var,
                          width=86, height=22, font=self.f_small, corner_radius=6,
                          fg_color=C_BTN, button_color=C_BTN_HOVER, button_hover_color="#d5d9df",
                          text_color=C_MUTED, dropdown_fg_color=C_CARD, dropdown_text_color=C_TEXT,
                          dropdown_hover_color=C_ACCENT_SOFT).pack(side="right", padx=(4, 0))
        ctk.CTkEntry(tools, textvariable=self.log_search_var, placeholder_text="搜索...", font=self.f_small,
                     width=150, height=24, corner_radius=6, border_width=1,
                     border_color=C_INPUT_BORDER, fg_color="white").pack(side="right", padx=(4, 0))

        log_holder = ctk.CTkFrame(card, fg_color=C_LOG_BG, corner_radius=10)
        log_holder.pack(fill="both", expand=True, padx=14, pady=(4, 12))
        self.log_text = tk.Text(log_holder, font=("Consolas", 11), state=tk.DISABLED, wrap=tk.WORD,
                                bg=C_LOG_BG, fg=C_LOG_FG, insertbackground=C_LOG_FG,
                                selectbackground="#264f78", relief=tk.FLAT, bd=0,
                                highlightthickness=0, padx=12, pady=10)
        self.log_text.pack(side="left", fill="both", expand=True, padx=(4, 0), pady=4)
        scrollbar = ctk.CTkScrollbar(log_holder, command=self.log_text.yview,
                                     button_color="#30363d", button_hover_color="#484f58",
                                     fg_color=C_LOG_BG)
        scrollbar.pack(side="right", fill="y", padx=(0, 4), pady=4)
        self.log_text.config(yscrollcommand=scrollbar.set)
        for level, color in LOG_LEVEL_COLORS.items():
            self.log_text.tag_configure(level, foreground=color)
        self.log_text.tag_configure("ts", foreground=C_LOG_MUTED)
        self.log_text.tag_configure("msg", foreground=C_LOG_FG)
        self._on_log("欢迎使用 GitHub 加速下载器，粘贴链接即可开始", "INFO")

    # ════════════════ 链接相关 ════════════════

    def _on_url_changed(self, *_):
        # 防抖：停止输入 500ms 后才解析，避免逐字输入时反复触发
        if self._url_parse_job:
            self.root.after_cancel(self._url_parse_job)
        self._url_parse_job = self.root.after(500, lambda: self._parse_url(show_warning=False))

    def _parse_url(self, show_warning: bool = True):
        if self._url_parse_job:
            self.root.after_cancel(self._url_parse_job)
            self._url_parse_job = None

        url = self.url_var.get().strip()
        if not url:
            if show_warning:
                messagebox.showwarning("提示", "请输入 GitHub 链接")
            return

        parsed = parse_github_url(url)
        if parsed.link_type == GitHubLinkType.UNKNOWN:
            self._on_log(f"无法识别的链接: {url}", "WARN")
            self.link_type_var.set(TYPE_META[GitHubLinkType.UNKNOWN])
            if show_warning:
                messagebox.showwarning("提示", "无法识别此 GitHub 链接类型")
            return

        self.parsed = parsed
        label = TYPE_META[parsed.link_type]
        if parsed.owner and parsed.repo:
            label += f" · {parsed.owner}/{parsed.repo}"
        if parsed.tag:
            label += f" @{parsed.tag}"
        elif parsed.branch:
            label += f" @{parsed.branch}"
        self.link_type_var.set(label)
        if parsed.suggested_filename:
            self.filename_var.set(parsed.suggested_filename)
        self._on_log(f"已解析: {label}", "INFO")
        self._fetch_size_async()

    def _apply_preset(self, url: str):
        self.url_var.set(url)
        self._parse_url(show_warning=False)

    def _clear_url(self):
        self.url_var.set("")
        self.parsed = None
        self.estimated_size = None
        self.link_type_var.set("未解析")
        self.size_var.set("预估大小: --")

    def _paste_url(self):
        try:
            clip = self.root.clipboard_get()
            self.url_var.set(clip.strip())
            self._on_log("已从剪贴板粘贴链接", "INFO")
            self._parse_url(show_warning=False)
        except tk.TclError:
            self._on_log("剪贴板为空或无法读取", "WARN")

    def _fetch_size_async(self):
        """后台 HEAD 请求获取资源大小，用于“预估大小”展示。"""
        if not self.parsed:
            return
        self._size_job_gen += 1
        gen = self._size_job_gen
        url = build_download_url(self.engine_id, self.parsed.download_url, self.ghproxy_prefix, self.kkgithub_host)
        proxies = get_proxies(self.engine_id, self.proxy_port)

        def worker():
            size = None
            try:
                resp = requests.head(url, proxies=proxies, timeout=10, allow_redirects=True)
                if resp.ok and resp.headers.get("Content-Length", "").isdigit():
                    size = int(resp.headers["Content-Length"])
            except (requests.RequestException, ValueError):
                pass
            self.root.after(0, lambda: self._on_size_fetched(size, gen))

        threading.Thread(target=worker, daemon=True).start()

    def _on_size_fetched(self, size: int | None, gen: int):
        if gen != self._size_job_gen or size is None:
            return
        self.estimated_size = size
        self.size_var.set(f"预估大小: 约 {self._format_size(size)}")
        if not self.running:
            self.transferred_var.set(f"0 B / {self._format_size(size)}")

    # ════════════════ 保存目录 ════════════════

    def _quick_dirs(self):
        home = os.path.expanduser("~")
        return [
            ("默认目录", os.getcwd()),
            ("下载目录", os.path.join(home, "Downloads")),
            ("桌面", os.path.join(home, "Desktop")),
        ]

    def _set_quick_dir(self, path: str):
        if os.path.isdir(path):
            self.save_dir_var.set(path)
        else:
            self._on_log(f"目录不存在: {path}", "WARN")

    def _choose_dir(self):
        d = filedialog.askdirectory(initialdir=self.save_dir_var.get())
        if d:
            self.save_dir_var.set(d)

    def _on_toggle_auto_open(self):
        if not save_config(auto_open=self.auto_open_var.get()):
            self._on_log("配置保存失败，设置不会被记住", "WARN")

    def _update_save_hints(self):
        save_dir = self.save_dir_var.get()
        filename = self.filename_var.get().strip()
        if "." in filename:
            self.ext_var.set("." + filename.rsplit(".", 1)[-1].upper()[:6])
        else:
            self.ext_var.set("")
        self.path_preview_var.set(os.path.join(save_dir, filename) if filename else "")
        try:
            free = shutil.disk_usage(save_dir).free
            self.disk_free_var.set(f"可用: {self._format_size(free)}")
        except OSError:
            self.disk_free_var.set("可用: --")

    # ════════════════ 代理与通道 ════════════════

    def _auto_detect_proxy(self):
        self._on_log("正在检测 Clash 代理端口...", "INFO")
        self._run_proxy_check()

    def _run_proxy_check(self, reset_status: bool = False):
        """后台检测代理并刷新状态：先验证当前端口，失败再全量扫描；完成后定时重检。"""
        self._check_gen += 1
        gen = self._check_gen
        try:
            port = int(self.port_var.get().strip())
        except ValueError:
            port = None

        def worker():
            result = None
            if port and check_port_open(port):
                result = (port, ping_proxy(port))
            if result is None:
                detected = detect_clash_proxy()
                result = (detected, ping_proxy(detected)) if detected else (None, None)
            if gen == self._check_gen:
                self.root.after(0, lambda: self._on_proxy_checked(result[0], result[1], reset_status))
                self.root.after(self.PROXY_CHECK_INTERVAL_MS, self._run_proxy_check)

        threading.Thread(target=worker, daemon=True).start()

    def _on_proxy_checked(self, port: int | None, latency: int | None, reset_status: bool = False):
        if port:
            if self.port_var.get().strip() != str(port):
                self.port_var.set(str(port))
                save_config(proxy_port=port)
            self.proxy_port = port
            self.proxy_latency = latency
            self.proxy_dict = {"http": f"http://127.0.0.1:{port}", "https": f"http://127.0.0.1:{port}"}
            if not self._proxy_online:
                lat = f"（延迟: {latency}ms）" if latency else ""
                self._on_log(f"检测到 Clash 代理已启动于 127.0.0.1:{port}{lat}，网络就绪", "SUCCESS")
            self._proxy_online = True
        else:
            self.proxy_port = None
            self.proxy_dict = None
            self.proxy_latency = None
            if self._proxy_online:
                self._on_log("代理连接已断开，请检查 Clash 是否还在运行", "WARN")
            elif not self._proxy_check_ran:
                self._on_log("未检测到 Clash 代理，请确认 Clash 正在运行，将自动重试", "WARN")
            self._proxy_online = False
        self._proxy_check_ran = True
        self._update_clash_indicators()
        if reset_status:
            self.progress_status_var.set("就绪")

    def _test_proxy_connection(self):
        try:
            port = int(self.port_var.get().strip())
        except ValueError:
            messagebox.showwarning("提示", "请输入有效的端口号")
            return
        self._on_log(f"测试端口 {port} 的代理连接...", "INFO")

        def worker():
            latency = ping_proxy(port)
            self.root.after(0, lambda: self._on_proxy_tested(port, latency))

        threading.Thread(target=worker, daemon=True).start()

    def _on_proxy_tested(self, port: int, latency: int | None):
        if latency is not None:
            self._on_proxy_checked(port, latency)
            self._on_log(f"代理连接成功 (端口 {port}，延迟 {latency}ms)", "SUCCESS")
        else:
            self._on_log(f"代理连接失败 (端口 {port})，请检查 Clash 是否运行", "ERROR")
        self.progress_status_var.set("就绪")

    def _set_quick_port(self, port: int):
        self.port_var.set(str(port))
        self._run_proxy_check()

    def _measure_channels_async(self):
        def worker():
            for channel in CHANNELS:
                ms = measure_latency(channel.id, self.proxy_port, self.ghproxy_prefix, self.kkgithub_host)
                self.root.after(0, lambda cid=channel.id, m=ms: self._on_channel_latency(cid, m))

        threading.Thread(target=worker, daemon=True).start()

    def _on_channel_latency(self, engine_id: str, latency: int | None):
        self.channel_latency[engine_id] = latency
        if engine_id == "clash" and latency is not None and self.proxy_latency is None:
            self.proxy_latency = latency
            self._update_clash_indicators()
        var = self.channel_latency_vars[engine_id]
        if latency is None:
            var.set("不可用")
        else:
            var.set(f"约 {latency}ms")

    def _select_engine(self, engine_id: str):
        if engine_id != self.engine_id:
            self.engine_id = engine_id
            self._on_log(f"已切换加速通道: {get_channel(engine_id).name}", "INFO")
            save_config(engine=engine_id)
        self._apply_engine_visual()
        self._fetch_size_async()

    def _apply_engine_visual(self):
        for cid, frame in self.channel_frames.items():
            selected = cid == self.engine_id
            frame.configure(fg_color=C_ACCENT_SOFT if selected else C_CARD,
                            border_color=C_ACCENT if selected else C_BORDER)

    # ════════════════ 下载控制 ════════════════

    def _on_primary_click(self):
        if self.running:
            if self.paused:
                self.downloader.resume()
                self.paused = False
                self.primary_btn.configure(text="暂停")
                self.progress_status_var.set("下载中...")
            else:
                self.downloader.pause()
                self.paused = True
                self.primary_btn.configure(text="继续")
                self.progress_status_var.set("已暂停")
        else:
            self._start_download()

    def _start_download(self):
        url = self.url_var.get().strip()
        if not url:
            messagebox.showwarning("提示", "请输入 GitHub 链接")
            return
        parsed = parse_github_url(url)
        if parsed.link_type == GitHubLinkType.UNKNOWN:
            messagebox.showwarning("提示", "无法识别此 GitHub 链接类型")
            return

        engine = self.engine_id
        proxies = get_proxies(engine, self.proxy_port)
        if engine == "clash" and proxies is None:
            self._on_log("未检测到 Clash 代理，已自动切换备用 GHProxy 镜像通道", "WARN")
            engine = "ghproxy"
            self._select_engine(engine)

        save_dir = self.save_dir_var.get()
        filename = self.filename_var.get().strip() or parsed.suggested_filename or "downloaded_file"
        save_path = os.path.join(save_dir, filename)
        final_url = build_download_url(engine, parsed.download_url, self.ghproxy_prefix, self.kkgithub_host)

        save_config(proxy_port=int(self.port_var.get().strip()) if self.port_var.get().strip().isdigit() else None,
                    save_dir=save_dir)
        self._on_log(f"使用通道: {get_channel(engine).name}", "INFO")
        self._on_log(f"下载地址: {final_url}", "INFO")
        self._on_log(f"保存路径: {save_path}", "INFO")

        self.running = True
        self.paused = False
        self.primary_btn.configure(text="暂停")
        self.cancel_btn.configure(state="normal")
        self.progress_bar.set(0)
        self.pct_var.set("0.0%")
        self.speed_var.set("--")
        self.transferred_var.set(f"0 B / {self._format_size(self.estimated_size)}" if self.estimated_size else "-- / --")
        self.eta_var.set(f"-- · {self.THREAD_LABEL}")
        self.progress_status_var.set("下载中...")

        def worker():
            dl = Downloader(
                proxy_dict=proxies,
                progress_callback=lambda p: self.root.after(0, lambda: self._update_progress(p)),
                log_callback=lambda msg, level: self.root.after(0, lambda: self._on_log(msg, level)),
            )
            self.downloader = dl
            ok = dl.download(final_url, save_path)
            self.root.after(0, lambda: self._on_download_finished(ok, save_dir))

        threading.Thread(target=worker, daemon=True).start()

    def _cancel_download(self):
        if self.downloader:
            self.downloader.stop()

    def _on_download_finished(self, success: bool, save_dir: str):
        self.running = False
        self.paused = False
        self.primary_btn.configure(text="▶ 开始下载")
        self.cancel_btn.configure(state="disabled")
        self.downloader = None

        filename = self.filename_var.get().strip()
        self._record_history(filename, self.last_downloaded, self.url_var.get().strip(), success)
        if success:
            self.progress_status_var.set("下载完成")
            self.progress_bar.set(1.0)
            self.pct_var.set("100.0%")
            self.eta_var.set(f"完成 · {self.THREAD_LABEL}")
            self._on_log(f"任务完成: {filename}", "SUCCESS")
            if self.auto_open_var.get():
                try:
                    os.startfile(save_dir)
                except OSError as e:
                    self._on_log(f"无法打开目录: {e}", "WARN")
        else:
            self.progress_status_var.set("已取消 / 失败")
            self.eta_var.set(f"-- · {self.THREAD_LABEL}")

    def _update_progress(self, p: DownloadProgress):
        self.progress_bar.set(max(0.0, min(1.0, p.percentage / 100)))
        self.pct_var.set(f"{p.percentage:.1f}%")
        self.last_downloaded = p.downloaded
        self.speed_var.set(self._format_speed(p.speed) if p.speed > 0 else "--")
        total = p.total if p.total > 0 else self.estimated_size
        total_text = self._format_size(total) if total else "未知"
        self.transferred_var.set(f"{self._format_size(p.downloaded)} / {total_text}")
        if p.speed > 0 and total:
            self.eta_var.set(f"{self._format_eta((total - p.downloaded) / p.speed)} · {self.THREAD_LABEL}")

    def _open_dir(self):
        save_dir = self.save_dir_var.get()
        if os.path.isdir(save_dir):
            os.startfile(save_dir)
        else:
            self._on_log(f"目录不存在: {save_dir}", "WARN")

    # ════════════════ 任务记录 ════════════════

    def _record_history(self, name: str, size: int, url: str, ok: bool):
        self.history.append({
            "time": time.strftime("%Y-%m-%d %H:%M"),
            "name": name,
            "size": size,
            "url": url,
            "ok": ok,
        })
        self.history = self.history[-30:]
        save_config(history=self.history)

    def _show_history(self):
        win = ctk.CTkToplevel(self.root, fg_color=C_CARD)
        win.title("任务记录")
        win.geometry("640x420")
        win.attributes("-topmost", True)
        win.after(100, win.attributes, "-topmost", False)

        ctk.CTkLabel(win, text="任务记录", font=self.f_title, text_color=C_TEXT).pack(anchor="w", padx=16, pady=(14, 6))
        rows = ctk.CTkScrollableFrame(win, fg_color="transparent")
        rows.pack(fill="both", expand=True, padx=12)

        if not self.history:
            ctk.CTkLabel(rows, text="暂无下载记录", font=self.f_base, text_color=C_MUTED).pack(pady=30)
        for item in reversed(self.history):
            row = ctk.CTkFrame(rows, fg_color=C_BTN, corner_radius=8)
            row.pack(fill="x", pady=3)
            ctk.CTkLabel(row, text=item.get("time", ""), font=self.f_small, text_color=C_MUTED, width=120, anchor="w").pack(side="left", padx=(10, 6), pady=6)
            ctk.CTkLabel(row, text=item.get("name", ""), font=self.f_base, text_color=C_TEXT, anchor="w").pack(side="left", padx=4, pady=6)
            size = item.get("size")
            ctk.CTkLabel(row, text=self._format_size(size) if size else "--", font=self.f_small, text_color=C_MUTED).pack(side="right", padx=10, pady=6)
            ok = item.get("ok")
            ctk.CTkLabel(row, text="成功" if ok else "失败", font=self.f_small,
                         text_color=C_GREEN if ok else C_RED).pack(side="right", padx=4, pady=6)

        buttons = ctk.CTkFrame(win, fg_color="transparent")
        buttons.pack(fill="x", padx=14, pady=(6, 14))

        def clear_history():
            self.history = []
            save_config(history=self.history)
            for widget in rows.winfo_children():
                widget.destroy()
            ctk.CTkLabel(rows, text="暂无下载记录", font=self.f_base, text_color=C_MUTED).pack(pady=30)

        self._make_button(buttons, "清空记录", clear_history).pack(side="left")
        self._make_button(buttons, "关闭", win.destroy, kind="primary").pack(side="right")

    # ════════════════ 日志 ════════════════

    def _on_log(self, msg: str, level: str = "INFO"):
        ts = time.strftime("%H:%M:%S")
        self._log_records.append((ts, level, msg))
        if len(self._log_records) > 800:
            self._log_records = self._log_records[-600:]
        self.log_count_var.set(f"运行日志 ({len(self._log_records)})")
        if self._log_passes(ts, level, msg):
            self._insert_log_line(ts, level, msg)
        if self.autoscroll:
            self.log_text.see(tk.END)

    def _log_passes(self, ts: str, level: str, msg: str) -> bool:
        level_filter = self.log_level_var.get()
        if level_filter != "全部" and level != level_filter:
            return False
        keyword = self.log_search_var.get().strip().lower()
        if keyword and keyword not in msg.lower():
            return False
        return True

    def _insert_log_line(self, ts: str, level: str, msg: str):
        self.log_text.config(state=tk.NORMAL)
        self.log_text.insert(tk.END, f"[{ts}] ", "ts")
        self.log_text.insert(tk.END, f"[{level}] ", level)
        self.log_text.insert(tk.END, f"{msg}\n", "msg")
        self.log_text.config(state=tk.DISABLED)

    def _render_log(self):
        self.log_text.config(state=tk.NORMAL)
        self.log_text.delete("1.0", tk.END)
        for ts, level, msg in self._log_records:
            if self._log_passes(ts, level, msg):
                self._insert_log_line(ts, level, msg)
        self.log_text.config(state=tk.DISABLED)
        if self.autoscroll:
            self.log_text.see(tk.END)

    def _toggle_autoscroll(self):
        self.autoscroll = not self.autoscroll
        self.autoscroll_chip.configure(fg_color=C_ACCENT_SOFT if self.autoscroll else C_BTN,
                                       text_color=C_ACCENT if self.autoscroll else C_MUTED)

    def _copy_log(self):
        lines = [f"[{ts}] [{level}] {msg}" for ts, level, msg in self._log_records
                 if self._log_passes(ts, level, msg)]
        if lines:
            self.root.clipboard_clear()
            self.root.clipboard_append("\n".join(lines))
            self._on_log(f"已复制 {len(lines)} 条日志到剪贴板", "INFO")

    def _export_log(self):
        lines = [f"[{ts}] [{level}] {msg}" for ts, level, msg in self._log_records
                 if self._log_passes(ts, level, msg)]
        if not lines:
            messagebox.showinfo("提示", "当前没有可导出的日志")
            return
        path = filedialog.asksaveasfilename(defaultextension=".log", initialfile="downloader.log",
                                            filetypes=[("日志文件", "*.log"), ("所有文件", "*.*")])
        if path:
            with open(path, "w", encoding="utf-8") as f:
                f.write("\n".join(lines))
            self._on_log(f"日志已导出: {path}", "SUCCESS")

    def _clear_log(self):
        self._log_records = []
        self.log_count_var.set("运行日志 (0)")
        self._render_log()

    # ════════════════ 状态刷新 ════════════════

    def _update_clash_indicators(self):
        if self._proxy_online:
            lat = f" ({self.proxy_latency}ms)" if self.proxy_latency else ""
            self.net_status_var.set(f"代理正常 {lat}".strip())
            self.net_status_label.configure(text_color=C_GREEN)
        else:
            self.net_status_var.set("未连接")
            self.net_status_label.configure(text_color=C_RED)

    # ════════════════ 工具方法 ════════════════

    @staticmethod
    def _format_size(size) -> str:
        try:
            size = int(size)
        except (TypeError, ValueError):
            return "--"
        if size < 1024:
            return f"{size} B"
        if size < 1024 * 1024:
            return f"{size / 1024:.1f} KB"
        if size < 1024 * 1024 * 1024:
            return f"{size / (1024 * 1024):.1f} MB"
        return f"{size / (1024 * 1024 * 1024):.2f} GB"

    @staticmethod
    def _format_speed(speed: float) -> str:
        if speed < 1024:
            return f"{speed:.0f} B/s"
        if speed < 1024 * 1024:
            return f"{speed / 1024:.1f} KB/s"
        return f"{speed / (1024 * 1024):.2f} MB/s"

    @staticmethod
    def _format_eta(seconds: float) -> str:
        seconds = int(seconds)
        if seconds < 60:
            return f"{seconds}秒"
        if seconds < 3600:
            return f"{seconds // 60}分{seconds % 60:02d}秒"
        return f"{seconds // 3600}时{(seconds % 3600) // 60:02d}分"


def main():
    ctk.set_appearance_mode("light")
    # 独立的任务栏标识：避免与旧的 pinned 缓存/默认图标关联
    try:
        ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID("GitHubDownloader.1")
    except (OSError, AttributeError):
        pass
    root = ctk.CTk()
    GitHubDownloaderApp(root)
    root.mainloop()


if __name__ == "__main__":
    main()
