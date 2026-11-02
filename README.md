# GitHub 加速下载器

通过 Clash 本地代理、镜像节点或直连通道，加速下载 GitHub 资源与任意 http(s) 直链文件（官网安装包、CDN 文件等）的 Windows 桌面工具。

**当前正式版：[v2.1.0](../../releases)（Rust + egui）**，源码位于 [`v2/`](v2/)。v1.0（Python）已归档至 [`legacy/`](legacy/)，仅作回退保留。

## 功能

- **任意直链下载**：不只 GitHub——官网安装包、CDN 文件等 http(s) 直链均可下载；GitHub 链接自动识别 Release 资产包 / Raw 文件 / blob 文件页 / 仓库压缩包 / 仓库主页，外部直链自动提取文件名
- **许可门槛站点自动确认**：SEGGER 等需勾选许可条款的下载链接，自动提交确认后获取真实文件
- **5 条加速通道**：Clash 本地代理、GHProxy 镜像、KKGitHub 节点、Moeyy 加速源、官方直连，点击切换并自动测速；镜像通道仅加速 GitHub 链接，外部直链自动降级为本地代理或直连
- **多线程分段下载**：大文件自动分 8 段并行（可配 4~32），支持跨重启断点续传（`.part` 分段文件）
- **断点续传 + 自动重试**：连接中断后从已下载位置自动续传（最多 7 次），30 秒无响应自动判中断重试，已完整的文件不会重复下载
- **预估大小 / 实时速度 / 剩余时间 / 速度波动曲线**
- **任务记录**：最近 30 次下载的本地历史
- **运行日志**：级别着色（INFO/SUCCESS/WARN/ERROR/PROXY）、关键字搜索、级别过滤、复制、导出
- **深浅色主题**、**剪贴板自动监测**、**下载完成提示音**、**单实例运行**
- **配置持久化**：兼容 v1 的 config.json
- 单文件 EXE（约 8.7 MB，免安装）

## 使用

需要 Rust 工具链（rustup 安装 1.85+）：

```bat
cd v2
cargo build --release
target\release\GitHubDownloader.exe
```

国内网络若依赖拉取失败：项目已内置 rsproxy 镜像配置（`v2/.cargo/config.toml`，海外环境可删）；仍失败时给 cargo 挂上本地代理环境变量（如 Clash）`set HTTPS_PROXY=http://127.0.0.1:7897` 后重试。

## 配置文件

程序同目录下的 `config.json`（首次修改设置后自动生成），v1/v2 通用：

```json
{
  "proxy_port": 7897,
  "save_dir": "D:/Downloads",
  "engine": "clash",
  "auto_open": false,
  "history": [],
  "segments": 8
}
```

| 字段 | 说明 |
|---|---|
| `proxy_port` | Clash 混合端口，也可在界面中自动检测 |
| `save_dir` | 默认保存目录 |
| `engine` | 加速通道：`clash` / `ghproxy` / `kkgithub` / `moeyy` / `direct` |
| `auto_open` | 下载完成后自动打开文件目录 |
| `history` | 任务记录（最多 30 条） |
| `segments` | 分段下载并发数 |
| `theme_dark` / `auto_clipboard` / `sound_enabled` / `timeout_seconds` | v2 新增偏好项 |

## 项目结构

```
v2/                 v2.x 正式版（Rust + egui）
  src/core/         与界面无关的核心库（解析/通道/代理/下载器/配置）
  src/ui/           主题、矢量图标与通用组件
  src/app.rs        界面状态机与各卡片
  src/events.rs     界面与后台线程的事件契约
legacy/             v1.0（Python）归档，仅作回退保留
  main.py           v1 界面与主程序
  build.py          v1 PyInstaller 打包脚本
```

v1 回退方式：`cd legacy` 后按原方式运行（需 Python 3.10+ 与依赖安装，见 `legacy/requirements.txt`）。

## 许可证

[MIT](LICENSE)
