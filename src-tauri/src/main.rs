//! 极涌 VortexDL v3 桌面入口（Tauri 2）。
//!
//! `--headless <url> [out]`：无界面冒烟下载，退出码报告成败（见 lib::run_smoke_headless）。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--headless") {
        let url = args.get(1).cloned().unwrap_or_default();
        if url.is_empty() {
            eprintln!("用法: VortexDL.exe --headless <url> [输出路径]");
            std::process::exit(2);
        }
        let out = args.get(2).map(std::path::PathBuf::from);
        let code = vortexdl_lib::run_smoke_headless_block(url, out);
        std::process::exit(code);
    }
    vortexdl_lib::run()
}
