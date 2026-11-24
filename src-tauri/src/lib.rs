//! 极涌 VortexDL v3 应用库：插件注册、命令挂载与 IPC 契约（阶段 C8）。
//!
//! IPC 面向前端（Vue）：
//! - Commands（`commands` 模块）：任务增删查/暂停/恢复/取消、设置读写、事件订阅、v2 配置导入
//! - Channel：任务事件流（引擎 Log/Progress/BtProgress/Finished，广播转发）
//! - `--headless <url> [out]`：无界面冒烟入口（main.rs 分发，见 [`run_smoke_headless`])

pub mod commands;
pub mod core;
pub mod engine;
pub mod store;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use engine::bt::BtEngine;
use engine::manager::TaskManager;
use store::{NewTask, Store};
use tokio::sync::OnceCell;

/// 应用共享状态（Tauri managed state）。
pub struct AppState {
    pub store: Store,
    pub manager: Arc<TaskManager>,
    /// BT 会话槽：与 TaskManager 共用，保证应用只有一个 BT 会话
    pub bt: OnceCell<Arc<BtEngine>>,
}

impl AppState {
    /// 打开数据目录（创建子目录与数据库），v2 迁移由前端按需调 [`commands::import_v2_config`]。
    pub async fn open(data_dir: &Path) -> anyhow::Result<Arc<Self>> {
        std::fs::create_dir_all(data_dir)?;
        let store = Store::open(&data_dir.join("vortexdl.db")).await?;
        store.reset_stale_downloading().await?;
        let manager = TaskManager::new(store.clone(), 3, OnceCell::new());
        let bt = manager.bt_slot();
        Ok(Arc::new(Self { store, manager, bt }))
    }

    /// 惰性创建 BT 会话（读设置）。
    pub async fn bt_engine(&self) -> Result<Arc<BtEngine>, String> {
        if let Some(e) = self.bt.get() {
            return Ok(e.clone());
        }
        let default_dir = self
            .store
            .get_setting("save_dir")
            .await
            .ok()
            .flatten()
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| ".".into());
        let port = self
            .store
            .get_setting("bt_listen_port")
            .await
            .ok()
            .flatten()
            .and_then(|s| s.parse::<u16>().ok());
        let enable_dht = self
            .store
            .get_setting("bt_enable_dht")
            .await
            .ok()
            .flatten()
            .and_then(|s| s.parse::<bool>().ok())
            .unwrap_or(true);
        let enable_upnp = self
            .store
            .get_setting("bt_enable_upnp")
            .await
            .ok()
            .flatten()
            .and_then(|s| s.parse::<bool>().ok())
            .unwrap_or(true);
        let engine = BtEngine::new(PathBuf::from(default_dir), port, enable_dht, enable_upnp)
            .await
            .map_err(|e| format!("BT 会话创建失败: {e}"))?;
        Ok(self.bt.get_or_init(move || async move { engine }).await.clone())
    }
}

/// 设置默认值（前端打底 + 存储覆盖）。
pub fn default_settings() -> std::collections::HashMap<String, String> {
    let c = core::config::Config::default();
    // save_dir 默认取用户 Downloads 目录（Config::default 的 save_dir 为空串，不能当真实路径用）
    let fallback_save_dir = std::env::var("USERPROFILE")
        .map(|p| format!("{p}\\Downloads"))
        .unwrap_or_else(|_| c.save_dir.clone());
    let mut m = std::collections::HashMap::new();
    m.insert("proxy_port".into(), c.proxy_port.to_string());
    m.insert("save_dir".into(), fallback_save_dir);
    m.insert("engine".into(), "direct".into());
    m.insert("auto_open".into(), c.auto_open.to_string());
    m.insert("ghproxy_prefix".into(), c.ghproxy_prefix.clone());
    m.insert("kkgithub_host".into(), c.kkgithub_host.clone());
    m.insert("segments".into(), c.segments.to_string());
    m.insert("timeout_seconds".into(), c.timeout_seconds.to_string());
    m.insert("classify_enabled".into(), "true".into());
    m.insert("global_limit_bps".into(), "0".into());
    m.insert("bt_enable_dht".into(), "true".into());
    m.insert("bt_enable_upnp".into(), "true".into());
    m.insert("browser_takeover_enabled".into(), "true".into());
    m.insert("browser_takeover_port".into(), "24556".into());
    m.insert("browser_takeover_token".into(), String::new());
    m
}

// ────────────────────────── headless 冒烟 ──────────────────────────

/// `--headless` 阻塞包装（main.rs 调用；内部起多线程运行时）。
pub fn run_smoke_headless_block(url: String, out: Option<PathBuf>) -> i32 {
    match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => rt.block_on(run_smoke_headless(url, out)),
        Err(e) => {
            eprintln!("[headless] 运行时创建失败: {e}");
            2
        }
    }
}

/// `--headless <url> [out]`：无界面下载一个 HTTP 资源，退出码报告成败。
/// 内存库 + 下载管理器直连，不复用 GUI 状态。
pub async fn run_smoke_headless(url: String, out: Option<PathBuf>) -> i32 {
    let store = match Store::open_memory().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[headless] 数据库初始化失败: {e}");
            return 2;
        }
    };
    let manager = TaskManager::new(store.clone(), 2, OnceCell::new());
    let mut rx = manager.subscribe();

    let (dir, name) = match &out {
        Some(p) => (
            p.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(".")),
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "headless-download.bin".into()),
        ),
        None => {
            let name = url
                .rsplit('/')
                .next()
                .unwrap_or("headless-download.bin")
                .split('?')
                .next()
                .unwrap_or("headless-download.bin")
                .to_string();
            (std::env::temp_dir(), name)
        }
    };

    let id = match manager
        .enqueue(NewTask {
            kind: store::TaskKind::Http,
            name: name.clone(),
            save_dir: dir.to_string_lossy().into_owned(),
            category: String::new(),
            urls: vec![url.clone()],
            segments: Some(core::config::DEFAULT_SEGMENTS as u32),
            limit_bps: None,
            use_clash: false,
        })
        .await
    {
        Ok(id) => id,
        Err(e) => {
            eprintln!("[headless] 建任务失败: {e}");
            return 2;
        }
    };

    println!("[headless] 开始下载: {url}");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
    loop {
        if std::time::Instant::now() > deadline {
            eprintln!("[headless] 超时（10 分钟）");
            return 3;
        }
        match tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv()).await {
            Ok(Ok((tid, ev))) if tid == id => match ev {
                engine::TaskEvent::Finished { ok, reason } => {
                    if ok {
                        let t = store.get_task(&id).await.ok().flatten();
                        let size = t.as_ref().map(|t| t.received_bytes).unwrap_or(0);
                        let p = t
                            .as_ref()
                            .map(|t| Path::new(&t.save_dir).join(&t.name))
                            .unwrap_or_default();
                        println!("[headless] 完成: {} 字节 → {}", size, p.display());
                        return 0;
                    }
                    eprintln!("[headless] 失败: {reason}");
                    return 1;
                }
                engine::TaskEvent::Progress(p) => {
                    println!("[headless] {:.1}%  {:.2} MB/s", p.pct, p.speed_bps / 1024.0 / 1024.0);
                }
                engine::TaskEvent::Log(level, msg) => {
                    println!("[headless] [{level:?}] {msg}");
                }
                engine::TaskEvent::BtProgress(_) => {}
                engine::TaskEvent::Status(_) => {}
            },
            Ok(_) => {}
            Err(_) => continue,
        }
    }
}

// ────────────────────────── Tauri 应用装配 ──────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // 第二实例启动时唤起已有窗口（对齐 v2 单实例语义）
            use tauri::Manager;
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.unminimize();
                let _ = win.set_focus();
            }
        }))
        .setup(|app| {
            use tauri::Manager;
            let dir = app.path().app_data_dir()?;
            let state = tauri::async_runtime::block_on(AppState::open(&dir))
                .map_err(|e| anyhow::anyhow!("应用状态初始化失败: {e}"))?;
            app.manage(state.clone());
            // 浏览器接管监听（127.0.0.1，端口走设置，默认 24556）
            tauri::async_runtime::spawn(async move {
                let port = engine::takeover::takeover_port(&state).await;
                if let Err(e) = engine::takeover::run_listener(state.clone(), port).await {
                    eprintln!("[takeover] 监听启动失败: {e}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_tasks,
            commands::create_http_task,
            commands::create_bt_task,
            commands::update_task,
            commands::restart_task,
            commands::pause_task,
            commands::resume_task,
            commands::cancel_task,
            commands::remove_task,
            commands::open_task_dir,
            commands::open_task_file,
            commands::get_settings,
            commands::set_setting,
            commands::import_v2_config,
            commands::subscribe_events,
            commands::detect_clash_port,
            commands::test_clash_proxy,
        ])
        .run(tauri::generate_context!())
        .expect("VortexDL 启动失败");
}
