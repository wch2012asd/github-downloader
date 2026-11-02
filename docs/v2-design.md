# GitHub 加速下载器 v2.0 设计文档（Rust 重构）

> 状态：已实施（2026-10-13 完成主体开发；与设计的差异见 §11 实施差异）
> 前置版本：v1.0（Python + CustomTkinter，commit cc9b31e）

---

## 1. 背景与目标

v1.0 已实现完整的下载闭环，但存在三类重构动机：

1. **分发体积与启动速度**：PyInstaller 单文件 21.7 MB，冷启动需解压、加载 Python 运行时（秒级）；Rust 原生编译预期 5~10 MB、毫秒级启动。
2. **性能天花板**：v1 为单线程下载（界面上 `1T` 标签已预留升级位），国内经镜像下载大文件时单流速度受限；v2 引入**多线程分段下载**作为头号新特性。
3. **工程质量**：v1 无测试、核心逻辑与 Tkinter 界面耦合（如 `_start_download` 内嵌解析逻辑）、异步结果用 `root.after` 手工同步、配置非原子写入。v2 将核心逻辑做成与 GUI 无关、可单测的库层。

**v2.0 总原则：功能对齐 v1 + 分段下载，界面布局与交互习惯保持一致，配置文件向后兼容。**

---

## 2. v1.0 功能梳理（代码级）

### 2.1 模块 × 功能清单

| 模块 | 功能点 | 关键参数/行为 |
|---|---|---|
| `url_parser.py` | 识别 4 类链接（Raw / Release 资产 / 仓库压缩包 / 仓库主页） | 仓库主页自动转 `main` 分支 zip；压缩包支持 `refs/heads`、`refs/tags`、简写 `{ref}.zip` |
| `engines.py` | 4 条加速通道：clash / ghproxy / kkgithub / direct | ghproxy 前缀拼接；kkgithub 域名替换（raw 域 → `raw.<主域名>`）；逐通道延迟测量（GET，超时 5s） |
| `downloader.py` | 单线程下载、断点续传、自动重试 | 分块 8KB；最多 8 次尝试（首下 + 7 重试），重试间隔 5s（可取消）；`Range` 续传；416 → HEAD 比对远端大小，一致则判"已完整"；期待 206 却收到 200 → 弃本地重下；进度每 0.5s 回调一次；HTTP 5xx 才值得重试 |
| `proxy.py` | Clash 检测 | 注册表读系统代理端口 → 常见端口列表（7890, 7897, 7891, 7892, 7899, 10809, 1080, 8080, 2080）逐个 TCP 探活（1s）→ 经代理 GET `api.github.com/zen` 验证（8s）；每 30s 周期复检 |
| `config.py` | config.json 持久化（EXE 同目录） | 字段：`proxy_port` / `save_dir` / `engine` / `auto_open` / `history`(≤30) / `ghproxy_prefix` / `kkgithub_host`；合并保存 |
| `main.py` | 双栏 5 卡片界面、下载控制、日志、历史 | 见 2.2 |
| `icons.py` | PIL 矢量绘制界面图标（24 网格 4 倍超采样） | link / folder / signal / gauge / terminal / clipboard / clock |
| `win32_window.py` | WM_SETICON + 类图标双通道设置窗口/任务栏图标 | — |
| `main()` | 任务栏独立标识 | `SetCurrentProcessExplicitAppUserModelID("GitHubDownloader.1")` |

### 2.2 界面与交互细节（v2 需保持的习惯）

- **窗口**：1280×820，最小 1120×720，浅色主题，双栏等宽，5 张圆角卡片。
- **左栏**：
  - 链接卡：URL 输入（等宽字体）+ 清空按钮 +「粘贴」+「解析 →」；预设 chips（LumaShot / Clash / VS）；输入防抖 500ms 自动解析；资源类型徽标（类型 · owner/repo @tag/branch）+ 预估大小（后台 HEAD 获取）。
  - 保存卡：目录快捷 chips（默认目录 / 下载目录 / 桌面）+「选择」对话框；文件名输入 + 扩展名徽标；完整路径预览；磁盘剩余空间；「下载完成后自动打开目录」勾选。
  - 网络卡：端口输入 +「自动检测」+「测试连接」+ 快捷端口 chips（7890/7891/10808）；4 张通道卡片（名称、延迟、标签徽标、点击切换、选中高亮蓝底）；代理状态文字（绿"代理正常 (xxms)" / 红"未连接"）。
- **右栏**：
  - 进度卡：进度条 + 完成度 / 当前速度 / 已传输÷总大小 / 剩余时间·线程 四项统计；主按钮（开始下载 ⇄ 暂停/继续）+ 取消 + 任务记录 + 打开目录。
  - 日志卡：终端风深色底；级别着色（INFO 蓝 / WARN 黄 / SUCCESS 绿 / ERROR 红）；关键字搜索、级别过滤、复制、导出 .log、清空、自动滚屏开关；上限 800 条（裁剪至 600）。
  - 历史弹窗：最近 30 条（时间 / 文件名 / 成败 / 大小），支持清空。
- **其他**：欢迎日志行；通道自动回退（选 clash 但未检测到代理 → 自动切 ghproxy 并提示）；完成时可选自动打开目录。

### 2.3 v1 已知局限（重构中一并解决/改善）

| # | 问题 | v2 对策 |
|---|---|---|
| L1 | 不识别 `github.com/{o}/{r}/blob/{branch}/{path}` 文件页链接（用户最常复制的形态之一） | 新增 blob → Raw 转换 |
| L2 | 不识别 `/tree/{branch}` 目录页、`.git` / SSH clone 地址 | 新增识别（tree → 该分支压缩包；clone 地址 → 转主页处理） |
| L3 | 单线程下载，镜像大文件速度受限 | 多线程分段下载（见 §6.2） |
| L4 | 续传只按本地文件大小，不校验内容是否同一资源（远端更新后会拼出坏文件） | 探测阶段记录 ETag，复用本地数据前比对 |
| L5 | 文件名未做 Windows 非法字符清洗 | 下载前 sanitize |
| L6 | 通道测速用完整 GET（镜像站会拉整个首页） | 改 HEAD 优先、GET 兜底 |
| L7 | 配置直接覆写，进程被杀可能写坏 | 临时文件 + rename 原子写 |
| L8 | 核心逻辑与 UI 耦合，无任何测试 | core 层与 GUI 分离，单测覆盖 |
| L9 | 分支名含 `/` 时 Raw 解析错误（v1 固定取第 3 段） | 已知 GitHub 语义限制，保持现状并在文档注明（无法与路径区分，属误接受范围） |
| L10 | 仓库主页一律假设 `main` 分支 | 探测失败时尝试 `master` 兜底 |

---

## 3. v2.0 范围

### 3.1 P0 — 功能对齐（验收即 v1 全量行为）

- [ ] 链接解析：v1 全部 4 类 + 新增 blob / tree / clone 地址
- [ ] 4 条加速通道 + 点击切换 + 逐通道测速 + 未开 Clash 自动回退 ghproxy
- [ ] Clash 代理：端口自动检测（注册表 + 常见端口扫描）、手动指定、测试连接、30s 周期复检
- [ ] 单任务下载：暂停 / 继续 / 取消、断点续传、自动重试（8 次、5s 间隔）、416 判完整、200 降级重下
- [ ] 预估大小、实时速度、剩余时间、已传输统计
- [ ] 界面 5 卡片布局与交互习惯对齐（§2.2 逐项）
- [ ] 日志面板：着色 / 搜索 / 过滤 / 复制 / 导出 / 清空 / 自动滚屏 / 800 上限
- [ ] 任务记录 ≤30 条 + 弹窗 + 清空
- [ ] config.json 向后兼容（能直接读 v1 配置），EXE 同目录
- [ ] 单文件 EXE（内嵌图标）、窗口/任务栏图标、任务栏独立标识（AUMID）

### 3.2 P1 — v2.0 新增

- [ ] **多线程分段下载**：默认 8 线程，可配 1~32；小文件（<8 MiB）或服务器不支持 Range 时自动单流
- [ ] 分段断点跨重启：`.part{i}` 文件 + ETag 校验，重开程序续传
- [ ] 下载前探测：HEAD 获取大小 / Accept-Ranges / ETag / Content-Disposition 文件名（无建议名时采用）
- [ ] 文件名 Windows 合法性清洗
- [ ] 自定义镜像前缀（界面可编辑 ghproxy 前缀，避免镜像站失效即瘫痪）
- [ ] 线程标签从 `1T` 变为实际并发数（如 `8T`）

### 3.3 P2 — 明确不做（v2.1 候选）

- 多任务下载队列 / 批量粘贴
- 暗色主题
- SHA-256 校验（GitHub API digest）
- 限速、SOCKS5 代理、「自动(最快通道)」
- 剪贴板自动监听

---

## 4. 技术选型

### 4.1 GUI 框架对比（✅ 已确认：egui/eframe）

> 2026-10 决策：采用 egui/eframe。egui 相对其他方案的两条决定性优势：即时模式无状态同步层，与后台下载线程的事件集成最简单；纯 MIT/Apache 许可最干净（Slint 需 GPL 或桌面免版税授权条款）。M0 里程碑仍包含中文输入法冒烟验证，不通过则切换 Slint（版本数据为 2026-10 crates.io 实查）。

| 方案 | 最新版本（维护状态） | 范式 | 中文 IME | 自定义样式 | EXE 体积 | 许可证 | 结论 |
|---|---|---|---|---|---|---|---|
| **egui + eframe** | 0.36.2（活跃） | 即时模式 | 可用，M0 冒烟 | ★★★ 自由绘制 | ~4-8 MB | MIT/Apache | ✅ 采用 |
| Slint | 1.18.1（活跃） | 声明式 .slint | 好，i18n 完善 | ★★★ 样式系统 | ~5-10 MB | GPL / 桌面免版税 | 预案 |
| iced | 0.14.0（更新偏慢） | Elm 架构（MVU） | 可用 | ★★ 需自写控件 | ~6-12 MB | MIT | 不选 |
| Tauri | 2.x 稳定（3.0 alpha） | Web 前端 + Rust 后端 | 浏览器级完美 | ★★★★ CSS | EXE 数 MB + 系统 WebView2 | MIT/Apache | 不选（架构错配） |
| Dioxus | 0.8 alpha（未到 1.0） | 类 React RSX | 跟随 webview | 跟随 webview | 类似 Tauri | MIT/Apache | 不选（不成熟） |
| gtk-rs / relm4 | relm4 0.11 | GTK 绑定 | 好 | 好 | Windows 需拖 GTK 运行时 | GPL 系 | 不选（Linux 优先） |
| native-windows-gui | 1.0.13（事实停更） | Win32 原生控件 | 系统级 | ★ 无法自绘卡片 | ~1-3 MB | MIT | 不选（停更） |

### 4.2 依赖清单（均锁定当前最新稳定版）

| 用途 | crate | 说明 |
|---|---|---|
| GUI | `eframe` / `egui` | 即时模式 |
| HTTP | `reqwest`（blocking，rustls-tls-native-roots） | **不启用 gzip 等解压特性**——下载工具必须拿到与 Content-Length 一致的原始字节，否则 Range/断点语义被破坏；走系统根证书，兼容企业代理 |
| 序列化 | `serde` + `serde_json` | 配置、分段元数据 |
| 注册表 | `winreg` | 系统代理读取 |
| Win32 少量调用 | `windows-sys` | AUMID（任务栏标识）、`GetDiskFreeSpaceExW`（磁盘余量） |
| 原生对话框 | `rfd` | 目录选择、日志导出另存为 |
| 剪贴板 | `arboard` | 「粘贴」按钮读取剪贴板（egui 内置剪贴板只覆盖文本框粘贴） |
| 图标 | `image` + `ico`（或手解） | 启动时载入内嵌 app_icon.ico 供 viewport 图标 |
| EXE 图标嵌入 | `winresource`（build.rs） | 替代 v1 的 PyInstaller `--icon` |
| 时间戳 | `chrono` | 历史记录 `%Y-%m-%d %H:%M` |
| 错误定义 | `thiserror` | core 层错误类型 |

**并发模型刻意不引入 tokio**：本工具同一时刻活跃的网络操作 ≤「1 个下载任务的 N 个分段 + 若干探测」，用 `std::thread` + 阻塞 reqwest 语义最直白（分段线程在 chunk 间隙查控制标志即可实现暂停/停止），编译更快、二进制更小。理由与备选（tokio + reqwest async）在评审时可讨论。

---

## 5. 架构设计

### 5.1 目录结构

v2 代码放 `v2/` 子目录，与 v1 并存直至 v2 达到对齐验收，届时再调整仓库顶层结构：

```
github-downloader/
├── main.py ...                # v1 保持不动
├── docs/v2-design.md          # 本文档
└── v2/
    ├── Cargo.toml
    ├── build.rs               # winresource 嵌入 EXE 图标
    ├── assets/app_icon.ico
    └── src/
        ├── main.rs            # 入口：eframe 启动、字体注册、图标、AUMID
        ├── app.rs             # App 结构：持有 core 句柄、每帧 drain 事件、路由到各卡片
        ├── events.rs          # UiEvent / Command 定义（GUI ↔ 后台契约）
        ├── ui/
        │   ├── mod.rs         # 双栏布局骨架
        │   ├── theme.rs       # v1 配色常量移植（C_* token）
        │   ├── widgets.rs     # Card / Chip / 通道卡片 / 统计格 等绘制辅助
        │   ├── link_card.rs
        │   ├── save_card.rs
        │   ├── network_card.rs
        │   ├── progress_card.rs
        │   ├── log_card.rs
        │   └── history.rs     # 历史弹窗
        └── core/              # ★ 与 GUI 无关，全部可单测
            ├── mod.rs
            ├── url_parser.rs  # 对应 url_parser.py + L1/L2 增强
            ├── engines.rs     # 对应 engines.py
            ├── proxy.rs       # 对应 proxy.py
            ├── downloader.rs  # 对应 downloader.py + 分段下载状态机
            ├── config.rs      # 对应 config.py + 原子写 + v1 兼容
            └── fmt.rs         # 大小/速度/ETA 格式化（v1 工具函数逐字移植）
```

### 5.2 线程与事件模型

```
┌─ UI 线程（egui）────────────────────────────────┐
│  每帧: event_rx.try_recv() 排空 → 更新状态 → 绘制  │
│  空闲时 request_repaint_after(100ms) 保持响应     │
└────────△───────────────────────┬────────────────┘
        │ UiEvent (mpsc)          │ Command（Arc 原子量 / 控制通道）
┌────────┴───────────────────────▽────────────────┐
│ 后台线程群（std::thread + 阻塞 reqwest）           │
│  · proxy_watch   30s 周期检测 → ProxyCheckDone    │
│  · prober        通道测速/预估大小 → ChannelLatency │
│                  / SizeFetched（带代数 gen 防串台） │
│  · task_worker   每个下载任务 1 个编排线程          │
│     └─ N 个分段线程（Range 各写各的 .part 文件）    │
└─────────────────────────────────────────────────┘
```

- **UiEvent**（后台 → UI）：
  `ProxyCheckDone{port,latency}` · `ChannelLatency{engine,ms}` · `SizeFetched{gen,size}` ·
  `Log{level,msg}`(时间戳由 UI 侧生成，与 v1 一致) · `Progress{gen,downloaded,total,speed_bps,pct}` ·
  `TaskFinished{gen,ok,reason}`
- **控制**（UI → 任务）：`Arc<TaskControl>` 内含 `Mutex<State>{Running,Paused,Stopping}` + Condvar；分段线程每写完一个 chunk（64KB）检查一次，暂停即安全退出，.part 文件长度天然记录进度；恢复时编排线程按各 part 现有长度重新派发剩余区间。
- **代数（generation）**：`AtomicUsize` 计数器用于预估大小、代理检测、下载任务，事件携带 gen，UI 丢弃过期事件——移植 v1 的 `_size_job_gen` / `_check_gen` 手法。
- **日志**：UI 侧 `Vec<(ts, level, msg)>` 环形缓冲（800→裁 600），日志面板按搜索/过滤条件在脏标记时重建 LayoutJob（避免每帧全量排版）。

### 5.3 关键数据结构（示意）

```rust
pub enum LinkType { RawFile, ReleaseAsset, RepoArchive, RepoPage, Unknown }

pub struct ParsedUrl {
    pub link_type: LinkType,
    pub owner: String, pub repo: String,
    pub branch: String, pub tag: String,
    pub download_url: String,
    pub suggested_filename: String,
}

pub struct Probe {            // 下载前探测结果
    pub total: Option<u64>,   // Content-Length / Content-Range
    pub accept_ranges: bool,
    pub etag: Option<String>,
    pub filename: Option<String>, // Content-Disposition
}

pub enum TaskState { Idle, Probing, Downloading, Paused, Retrying{attempt:u8}, Completed, Failed(String), Cancelled }
```

---

## 6. 模块设计

### 6.1 url_parser

- 保留 v1 全部规则，新增：

| 输入 | 识别为 | 动作 |
|---|---|---|
| `github.com/{o}/{r}/blob/{branch}/{path}` | Raw 文件 | 转 `raw.githubusercontent.com/{o}/{r}/{branch}/{path}` |
| `github.com/{o}/{r}/tree/{branch}[/dir]` | 仓库压缩包 | 下载该 ref 的 `archive/refs/heads/{branch}.zip`（建议名 `{repo}-{branch}.zip`） |
| `github.com/{o}/{r}.git`、`git@github.com:{o}/{r}.git` | 仓库主页 | 去后缀走主页规则 |

- 查询串/`#` 片段剥离；URL 百分号编码文件名解码后作为建议名。
- 全部为纯函数，表驱动单测。

### 6.2 downloader（核心状态机）

```
Probing ──► Planning ──► Downloading(分段) ──► Assembling ──► Completed
   │            │             │  ▲                │
   │            │             ▼  │ Retrying(≤8次)  │
   └──► Downloading(单流, v1 逻辑) ─────────────────┴──► Failed / Cancelled
```

**探测**：HEAD（10s 超时）→ 405/无 Content-Length 时改发 `GET Range: bytes=0-0` 读 206 的 `Content-Range` → 仍失败则按"未知大小单流"处理。记录 ETag、Accept-Ranges、Content-Disposition。

**分段规划**：`accept_ranges && total ≥ 8 MiB` 时启用；`N = clamp(配置, 1, 32)`，且 `N ≤ total / 1 MiB`（避免碎片段）。

**分段执行**：
- 落盘：`{save_path}.part0 … .part{i}`，每个分段线程独立文件句柄——**规避 Windows 多句柄写同一文件的共享冲突**，且 part 文件长度即进度，崩溃/重启后无需元数据即可恢复。
- 复用判定：启动时若发现同名 part 集，探测 ETag 与记录一致（ sidecar `{save_path}.ghdl.json` 存 url/etag/total/N ）→ 续传；不一致 → 清空重下。完成后合并（顺序 append 至最终文件）并删除 part 与 sidecar。
- 单流模式（小文件 / 不支持 Range）：完全沿用 v1 行为——直接写最终文件、按文件大小 Range 续传、416→HEAD 比对判完整、200 降级重下。
- 重试：per-segment，最多 8 次尝试、间隔 5s（等待期可取消），仅网络类错误与 5xx 重试；某段耗尽 → 整任务失败。
- 进度：各分段线程累加写入量到 `Arc<AtomicU64>`，编排线程每 500ms 汇总计算速度（EMA 平滑）发 `Progress` 事件。

**取消语义**：与 v1 一致——已写入数据保留，下次可续传。

### 6.3 engines / proxy

- 通道定义、URL 变换（ghproxy 前缀 / kkgithub 域名替换）逐字移植；`ghproxy_prefix` 增加界面编辑入口（P1）。
- 测速：clash/direct 用 `GET api.github.com/zen`；镜像通道改 **HEAD 优先**（L6），HEAD 被拒则 GET 兜底；不可用 → 显示「不可用」。
- 代理检测算法与 v1 相同：注册表端口优先 → 9 个常见端口扫描；`TcpStream::connect_timeout` 1s 探活 + 经代理 GET zen 8s 验证；30s 周期复检线程；结果经事件回 UI。

### 6.4 config

- 路径与字段完全兼容 v1（EXE 同目录 `config.json`）；v2 新增 `segments: u8 = 8`，未知字段透传保留。
- 原子写：写 `{path}.tmp` → `rename` 覆盖（L7）。
- 单测：v1 样例文件读取、roundtrip、坏 JSON 容错。

### 6.5 UI（egui 映射）

- **字体**：启动时从 `C:\Windows\Fonts\` 加载 `msyh.ttc`（微软雅黑 UI）与 `consola.ttf`（Consolas）注册进 egui `FontDefinitions`，等宽族（日志/URL）与比例族分开；加载失败弹日志告警并回退 egui 内置字体。
- **布局**：CentralPanel 内 `ui.columns(2, …)` 双栏；左栏三卡 / 右栏两卡，行高、圆角、配色 token 从 `theme.rs` 取（值照抄 v1 十六进制色）。
- **自绘组件**：Card（圆角 14 + 1px 边框 Frame）、Chip（小号可点击标签）、通道卡片（可选中 Frame，选中态蓝底蓝框）、扩展名徽标——均为 egui Frame + 按钮的组合，无需自绘 painter 的部分尽量用现成控件。
- **进度条**：egui `ProgressBar` 或 painter 自绘 8px 圆角条（对齐 v1 观感）。
- **对话框**：目录选择 / 导出另存为用 `rfd` 原生对话框；历史与提示类改为 egui 内嵌弹窗（不再有独立 messagebox 弹窗，提示写日志 + 状态行）。
- **图标**：`app_icon.ico` 经 `include_bytes!` 内嵌 → `image` 解码 32/16px → `ViewportBuilder::with_icon`（替代 v1 的 Win32 WM_SETICON 双通道 hack）；EXE 图标由 build.rs winresource 嵌入；AUMID 启动时经 windows-sys 设置。
- **粘贴按钮**：arboard 读剪贴板 → 填入 URL 输入框（等价 v1 行为）。

---

## 7. 测试策略

| 层 | 手段 |
|---|---|
| url_parser / fmt / engines 变换 / config | cargo test 表驱动单测（含 v1 config 样例兼容、非法输入） |
| downloader | 本地假服务器（std TcpListener 手写 HTTP 响应，零额外依赖）覆盖：206 续传、200 降级、416 判完整、5xx 重试、分段合并、part 复用与 ETag 失配清空 |
| proxy / engines 测速 | 仅单测 URL/代理参数拼装；真实网络行为手测 |
| GUI | 手测清单（§2.2 逐项过一遍）+ 中文 IME 冒烟（M0） |

---

## 8. 构建与发布

```bat
cd v2
cargo build --release        :: 产物 target\release\GitHubDownloader.exe（单文件，预期 5~10 MB）
```

- release profile：`lto = "thin"`、`strip = true`、`codegen-units = 1`（压体积）。
- 图标：build.rs winresource 嵌入 → 无需外部 ico 分发。
- 无需安装器；README 届时补 v2 构建说明。
- Defender/SmartScreen：原生 exe 误报概率远低于 PyInstaller onefile；代码签名不在本期范围。

---

## 9. 里程碑

| 里程碑 | 内容 | 验收标准 |
|---|---|---|
| M0 脚手架 | Cargo 工程、eframe 窗口、雅黑/Consolas 字体加载、图标嵌入、**中文 IME 冒烟** | 窗口显示中文、可中文输入、任务栏图标正确 |
| M1 core 库 | url_parser（含新链接类型）、engines、config、fmt + 全套单测 | `cargo test` 绿；v1 config.json 可读 |
| M2 下载引擎（单流） | 探测、单流下载、断点、重试、416/200 语义 + 假服务器集成测试 | 测试全绿；命令行 harness 可完整下载一个 Release 资产 |
| M3 分段下载 | 多线程分段、part 复用、暂停/恢复/取消、合并 | 大文件 8T 下载、杀进程重开可续传 |
| M4 主界面 | 5 卡片布局、解析、保存卡、下载控制、进度联动 | 交互对齐 §2.2 手测清单（链接/保存/进度部分） |
| M5 网络/日志/历史 | 代理检测与复检、通道测速切换回退、日志面板全功能、历史弹窗 | 手测清单全绿 |
| M6 发布 | release 编译调优、EXE 图标、README 更新、dist 产出 | 单 exe 可在他机（无 Python 环境）运行 |

---

## 10. 风险与对策

| 风险 | 对策 |
|---|---|
| egui 中文 IME 在 Windows 有历史问题 | M0 第一优先冒烟；不达标切 Slint（核心 core 层不受影响） |
| 系统字体缺失（极端精简系统） | 告警 + 内置字体回退选项（体积代价 ~3-5MB，仅必要时做） |
| 镜像站不支持 HEAD/Range | 探测降级链（HEAD→Range 0-0→盲下）；不支持 Range 自动单流 |
| 镜像站失效（kkgithub 等历史上有波动） | P1 自定义镜像前缀界面 |
| 多线程写文件冲突 | 每段独立 part 文件，从设计上规避 |
| reqwest 拉入 tokio 导致编译慢/体积涨 | 接受（仍远小于 Python 方案）；极端情况下可换 ureq |

---

## 11. 实施差异（2026-10-13 实施后回写）

以下为实施过程中相对本设计的实际调整，均已落在代码中：

| # | 设计 | 实际 | 原因 |
|---|---|---|---|
| 1 | TLS 用 rustls-tls-native-roots | reqwest `default-tls`（schannel） | Windows 独占目标：直接用系统证书库，体积更小、兼容企业代理 |
| 2 | eframe 默认特性 | `default-features=false` + 仅 `glow` 后端 | 默认会拉入 wgpu；glow-only 使 EXE 从 15.1MB 降至 8.5MB |
| 3 | blocking reqwest 的 read timeout（对齐 v1 的 30s 读间隔超时） | `StallReader`：独立线程阻塞读 + `recv_timeout(30s)` 等待 | blocking ClientBuilder 没有 read_timeout（仅 async 有），整体 timeout 会误杀长下载；超时后放弃读线程、按可重试中断处理 |
| 4 | UI 按 ui/link_card.rs 等分文件 | 卡片绘制集中在 app.rs 的方法 | egui 即时模式下卡片与应用状态耦合紧，拆文件只会制造借用摩擦 |
| 5 | P1：Content-Disposition 文件名自动采用 | 仅记日志提示（"服务器返回文件名: X"） | 下载中途改路径会导致界面显示与实际不一致；文件名仍以解析建议为准 |
| 6 | master 分支兜底 | 任务内探测 404 → 换备用地址重探测（含预估大小的双探测） | — |
| 7 | — | 变量命名避开 `gen` | 2024 edition 中 `gen` 为保留字（生成器），统一用 `task_gen` |
| 8 | — | eframe 0.36 API 适配记录 | `App::update` 拆为 `ui(&mut Ui, frame)`（必需）+ `logic(ctx, frame)`；`Context::style()` → `all_styles_mut`/`set_theme`；`Button::hover_color` 移除（悬停色走样式）；`rect_stroke` 需 `StrokeKind` |

### 交付物状态

- `v2/` Cargo 工程：debug/release 构建通过，release 8.5 MB（v1 PyInstaller 21.7 MB）
- 测试：40 个全部通过（fmt/url_parser/engines/config/proxy/downloader 单测 + 7 个假服务器集成测试，覆盖断点续传、416 判完整、200 降级、分段合并、分段跨取消续传、404 主备切换）
- GUI：五卡片布局、解析（含 blob/tree/clone 新链接）、下载控制（暂停/继续/取消）、进度联动、代理巡检（30s）、通道测速切换、日志面板（搜索/过滤/复制/导出/滚屏）、任务记录弹窗、文件名清洗、磁盘余量、master 兜底
- 待人工验收：中文输入法（egui IME 冒烟）、窗口/任务栏图标肉眼确认（M0 遗留的两项人工检查）

---

## 12. 界面重做（2026-10-13，对齐 AI Studio 设计稿）

用户提供 AI Studio 设计稿（React/TSX 源码已提取至 `docs/design-ref/`，14 个文件），界面按其全面重做：

### 新增能力
- **无边框自定义窗口**：`with_decorations(false)` + 自绘标题栏（品牌徽标/版本徽章/⚡引擎状态摘要/任务记录入口（带数量徽标）/设置/亮暗切换/最小化-最大化-关闭），支持拖拽移动、双击最大化、Windows 11 圆角；`docs/design-ref/` 中 FolderPickerModal 在桌面版以原生 rfd 对话框替代
- **亮 / 暗双主题**：暗色底 `#121417`（石板黑），日志终端两种主题下恒为 `#0a0a0a`；主题选择持久化到 config
- **图标系统**：`ui/icons.rs` 手绘 24 网格描边图标 ~37 枚（风格对齐设计稿所用 lucide）
- **第 5 条加速通道 Moeyy**（`https://github.moeyy.xyz/` 前缀）；ghproxy 默认前缀改为 `https://ghproxy.net/`（与设计稿一致）；通道卡片带描述悬停提示与未测速延迟提示
- **新日志级别 PROXY**（青蓝色，用于通道切换/引擎启用类事件）
- **速度波动曲线**：进度卡内 24 样本柱状迷你图 + 峰值显示
- **直链下载**：交给系统浏览器按当前通道加速链接下载（设计稿语义）
- **设置弹窗**：分段线程数（4/8/16/32）、网络请求超时（5-60s，接入探测/连接超时）、剪贴板自动监测（后台 1s 轮询、自动填入并解析）、完成提示音（系统音替代设计稿的柔和双音，避免引入音频依赖）
- **历史弹窗重构**：文件名/大小/时间/路径 + 每条「重下」动作；任务记录增加 save_path 字段（config 兼容，serde default）
- **底部状态栏**：存储 / 引擎 / 代理 | 分块传输 / 全屏切换
- **Toast 通知**：顶部居中 3 秒提示

### 保留的自有优势（设计稿为纯前端模拟，无此能力）
真实 HEAD 探测预估大小（设计稿为按扩展名估算）、真下载引擎（断点/分段/重试）、rfd 原生目录选择、master 分支兜底、文件名非法字符清洗。

### 已知字体差异
设计稿使用 Plus Jakarta Sans / JetBrains Mono；Windows 端沿用微软雅黑 + Consolas（等宽），观感近似且零分发成本。

### 验证
- cargo test 41/41 通过（新增 Moeyy URL 变换测试）
- debug/release 冒烟通过；release 8.7 MB

## 13. 界面修正（2026-10-13 第二轮，用户反馈 15 项）

1. 布局改为三个显式区域（标题栏/内容/状态栏），修复状态栏画到标题栏下方并遮挡卡片的问题
2. 标题栏拖拽改为官方 custom-window-frame 模式（内容绘制完成后整栏注册 interact）
3. 标题栏仅保留应用图标（内嵌 app_icon.ico 纹理）+ 名称 + 版本 + 窗口控制；任务记录/设置移入下载进度卡按钮组，主题切换移入设置弹窗，引擎状态摘要移入底部状态栏（含延迟与在线着色）
4. 移除外层滚动条：内容按 720p 最小窗口设计即可完整展示，日志卡与网络卡分别撑满左右栏剩余高度
5. 按钮（开始下载/取消/重下/直链下载/打开目录）统一为 92×32
6. 全局字号上调 1~2px（正文 13、统计 14、标题 13 强调）
7. 输入框边框可见（Inactive 视觉态显式描边）
8. 通道行间距统一由 item_spacing 提供（修复双倍间距导致的溢出），徽标完全收进卡片内
9. 去掉状态栏全屏按钮；设置/历史弹窗改回 ctx 承载（修复全宽畸形）；主题切换入设置弹窗

## 14. 界面修正（2026-10-14 第二轮反馈）

- 根因发现：用户屏幕为 125% 缩放（工作区约 1228×768 逻辑像素），窗口默认 1280×820 超出屏幕是此前"遮挡/对不齐"假象的来源；exe 补 PerMonitorV2 DPI 清单（winresource set_manifest），高分屏文字清晰
- 窗口调整为默认 1180×710、最小 1000×600，初始位置固定可见
- 标题栏：仅保留 应用图标+名称+版本 与 窗口控制（— □ ✕ 标准 Windows 顺序、图标按短边居中）+ 主题切换（窗口控制左侧、月牙双圆叠加绘制）；拖拽限定在按钮左侧安全区、拖拽开始瞬间一次性 StartDrag（修复抖动与按钮吞点击）
- 设置/任务记录移入下载进度卡（标题栏图标按钮），行动按钮行不再拥挤；主题切换移入标题栏后设置弹窗去重
- 设置/历史弹窗重排：去掉 egui 默认灰色标题条，改为与主窗口一致的自绘三段式（头/体/脚 + 主题令牌卡片框），换新窗口 Id 摆脱 eframe 持久化的旧尺寸
- 卡片间距显式化（6px），网络卡/日志卡精确撑满栏底（运行时测量剩余高度），两栏底边对齐
- 输入框 TextEdit 增加 vertical_align(Center) 修复文字垂直不居中；边框经 Inactive 视觉态显式描边
