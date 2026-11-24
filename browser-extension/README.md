# 极涌 VortexDL 浏览器下载接管扩展

将浏览器所有新下载自动转交给本机运行的 VortexDL 桌面端（多线程分块加速）。

## 安装（Chrome / Edge 通用，加载已解压扩展）

1. VortexDL 桌面端运行中
2. 打开扩展管理页：`chrome://extensions`（Edge 为 `edge://extensions`）
3. 开启「开发者模式」→「加载解压缩的扩展」→ 选择本 `browser-extension/` 文件夹

## 配置（两处缺一不可）

1. **站点访问权限（Edge 必手动开）**
   Edge 对 MV3 扩展的主机权限默认不授予：扩展详情页 →「站点访问权限」→ 打开 `http://127.0.0.1/*` 开关。
   （Chrome 一般安装时已自动授予；若不工作同样检查此处。）
2. **Token**
   扩展图标右键 →「选项」→ 粘贴 VortexDL「设置 → BitTorrent / P2P → 接管 Token」→ 保存。

## 验证

浏览器里随意触发一个下载：

- 浏览器下载条消失（下载被取消转交）
- VortexDL 任务列表出现该任务并开始多线程下载
- 扩展角标：✓ 绿=转交成功；! 红=失败（检查 VortexDL 是否运行、Token 是否一致）

## 说明

- 接口仅监听 127.0.0.1（端口/Token 在 VortexDL「设置 → BitTorrent / P2P」中）
- VortexDL 未运行时转交失败，浏览器下载不受影响（回退正常下载）
- 两类下载都接管：
  - 普通网络直链 → 转发 URL，由桌面端多线程下载
  - 页面内存文件（`blob:`，如 ChatGPT 生成的文件）→ 由 `content.js` 在页面内读取字节回传，桌面端直接落盘
  - `data:` 内联数据保持浏览器下载
- blob 接管仅对 `chatgpt.com` 生效（`manifest.json` 的 `content_scripts` 匹配）；页面若在读取前释放了 blob 地址，该次下载留在浏览器
- 扩展代码更新后需在 `edge://extensions` 重新加载，并刷新已打开的页面（内容脚本在页面加载时注入）
- Firefox 需单独适配 WebExtensions 差异，暂未提供
