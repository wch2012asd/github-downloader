//! 构建脚本：将应用图标嵌入 EXE 资源（对应 v1 PyInstaller 的 --icon）。

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // PerMonitorV2 DPI 感知：高分屏缩放下文字清晰、坐标不虚化
    winresource::WindowsResource::new()
        .set_icon("assets/app_icon.ico")
        .set_manifest(r##"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="GitHubDownloader" version="2.0.0.0"/>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>"##)
        .compile()
        .expect("嵌入 EXE 图标失败：请确认已安装 Visual Studio 生成工具（需要 rc.exe）");
}
