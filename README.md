# GitHub 加速下载器

通过 Clash 本地代理或公共镜像通道加速下载 GitHub 资源的 Windows 桌面小工具（Python + CustomTkinter）。

## 功能

- **链接自动识别**：Release 资产包 / Raw 文件 / 仓库压缩包（archive）/ 仓库主页（自动转 main 分支 ZIP），粘贴后自动解析并填好文件名
- **4 种加速通道**：Clash 本地代理、GHProxy 镜像、KKGitHub 节点、官方直连，点击切换并自动测速；未开启 Clash 时自动回退镜像通道
- **断点续传 + 自动重试**：连接中断后从已下载位置自动续传（最多 7 次），已完整的文件不会重复下载
- **预估大小 / 实时速度 / 剩余时间**
- **任务记录**：最近 30 次下载的本地历史
- **运行日志**：级别着色（INFO/WARN/SUCCESS/ERROR）、关键字搜索、级别过滤、复制、导出
- **配置持久化**：代理端口、保存目录、加速通道等自动记忆
- 单文件 EXE，免安装

## 使用

1. 从 [Releases](../../releases) 下载 `GitHubDownloader.exe`（或按下方步骤自行构建）
2. 粘贴 GitHub 链接 → 自动解析 → 开始下载
3. 开着 Clash 就用"Clash 本地代理"通道；没开 Clash 选"GHProxy 镜像"等镜像通道同样可以下载

## 配置文件

程序同目录下的 `config.json`（首次修改设置后自动生成）：

```json
{
  "proxy_port": 7890,
  "save_dir": "D:/Downloads",
  "engine": "clash",
  "auto_open": false
}
```

| 字段 | 说明 |
|---|---|
| `proxy_port` | Clash 混合端口，也可在界面中自动检测 |
| `save_dir` | 默认保存目录 |
| `engine` | 加速通道：`clash` / `ghproxy` / `kkgithub` / `direct` |
| `auto_open` | 下载完成后自动打开文件目录 |
| `history` | 任务记录（最多 30 条） |

## 从源码运行 / 构建

需要 Python 3.10+（开发环境为 3.13）。

```bat
python -m venv venv
venv\Scripts\pip install -r requirements.txt

venv\Scripts\python main.py      :: 运行
venv\Scripts\python build.py     :: 打包单文件 EXE（输出到 dist\）
```

## 项目结构

```
main.py            界面与主程序
downloader.py      下载引擎（断点续传、自动重试）
engines.py         加速通道定义、URL 变换与测速
proxy.py           Clash 代理检测（端口扫描 + 系统代理注册表）
url_parser.py      GitHub 链接解析
config.py          配置读写
icons.py           界面矢量图标绘制
win32_window.py    窗口/任务栏图标设置（Win32）
assets/            图标资源
build.py           PyInstaller 打包脚本
```

## 许可证

[MIT](LICENSE)
