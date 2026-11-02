# GitHub 加速下载器

通过 Clash 本地代理或公共镜像通道加速下载 GitHub 资源的 Windows 桌面小工具。

- **v2.0（Rust + egui，开发中）**：源码在 [`v2/`](v2/)，设计与实施说明见 [`docs/v2-design.md`](docs/v2-design.md)
- **v1.0（Python + CustomTkinter，正式版）**：源码在仓库根目录

## 功能

- **链接自动识别**：Release 资产包 / Raw 文件 / `/blob/` 文件页 / 仓库压缩包（archive、`/tree/`）/ 仓库主页（自动转 main 分支 ZIP，404 时兜底 master），粘贴后自动解析并填好文件名；兼容 `.git` / SSH 克隆地址
- **4 种加速通道**：Clash 本地代理、GHProxy 镜像、KKGitHub 节点、官方直连，点击切换并自动测速；未开启 Clash 时自动回退镜像通道
- **多线程分段下载（v2）**：大文件自动分 8 段并行（可配 1~32），支持跨重启断点续传（`.part` 分段文件）
- **断点续传 + 自动重试**：连接中断后从已下载位置自动续传（最多 7 次），已完整的文件不会重复下载；30 秒无响应自动判中断重试
- **预估大小 / 实时速度 / 剩余时间**
- **任务记录**：最近 30 次下载的本地历史
- **运行日志**：级别着色（INFO/WARN/SUCCESS/ERROR）、关键字搜索、级别过滤、复制、导出
- **配置持久化**：代理端口、保存目录、加速通道等自动记忆；v1 的 config.json 直接兼容
- 单文件 EXE，免安装

## 使用 v2.0（Rust 版）

需要 Rust 工具链（rustup 安装 1.85+）：

```bat
cd v2
cargo build --release
target\release\GitHubDownloader.exe
```

国内网络若依赖拉取失败：本项目已内置 rsproxy 镜像配置（`v2/.cargo/config.toml`，海外环境可删）；仍失败时给 cargo 挂上本地代理环境变量（如 Clash）`set HTTPS_PROXY=http://127.0.0.1:7897` 后重试。

## 使用 v1.0（Python 版）

```bat
python -m venv venv
venv\Scripts\pip install -r requirements.txt

venv\Scripts\python main.py      :: 运行
venv\Scripts\python build.py     :: 打包单文件 EXE（输出到 dist\）
```

## 配置文件

程序同目录下的 `config.json`（首次修改设置后自动生成），v1/v2 通用：

```json
{
  "proxy_port": 7890,
  "save_dir": "D:/Downloads",
  "engine": "clash",
  "auto_open": false,
  "segments": 8
}
```

| 字段 | 说明 |
|---|---|
| `proxy_port` | Clash 混合端口，也可在界面中自动检测 |
| `save_dir` | 默认保存目录 |
| `engine` | 加速通道：`clash` / `ghproxy` / `kkgithub` / `direct` |
| `auto_open` | 下载完成后自动打开文件目录 |
| `history` | 任务记录（最多 30 条） |
| `segments` | 分段下载并发数（仅 v2，默认 8） |

## 项目结构

```
main.py            v1 界面与主程序（Python）
downloader.py      v1 下载引擎（断点续传、自动重试）
engines.py         v1 加速通道定义、URL 变换与测速
proxy.py           v1 Clash 代理检测（端口扫描 + 系统代理注册表）
url_parser.py      v1 GitHub 链接解析
config.py          v1 配置读写
icons.py           v1 界面矢量图标绘制
win32_window.py    v1 窗口/任务栏图标设置（Win32）
assets/            图标资源
build.py           v1 PyInstaller 打包脚本
v2/                v2 Rust 版（egui）
  src/core/        与 GUI 无关的核心库（解析/通道/代理/下载器/配置）
  src/ui/          界面主题与通用组件
  src/app.rs       界面状态机与各卡片
docs/              设计文档
```

## 许可证

[MIT](LICENSE)
