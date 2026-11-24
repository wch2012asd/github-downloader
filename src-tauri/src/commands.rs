//! Tauri IPC 命令集（阶段 C8）。
//! 独立子模块：规避 crate 根部命令宏命名空间冲突（tauri 已知行为）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::State;

use crate::store::{NewTask, TaskKind};
use crate::{store, AppState};

#[tauri::command]
pub async fn list_tasks(state: State<'_, Arc<AppState>>) -> Result<Vec<store::TaskRecord>, String> {
    state.store.list_tasks().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_http_task(
    state: State<'_, Arc<AppState>>,
    urls: Vec<String>,
    name: String,
    save_dir: String,
    segments: Option<u32>,
    limit_bps: Option<u64>,
    use_clash: Option<bool>,
) -> Result<String, String> {
    if urls.is_empty() {
        return Err("至少需要一个下载地址".into());
    }
    state
        .manager
        .enqueue(NewTask {
            kind: TaskKind::Http,
            name,
            save_dir,
            category: String::new(),
            urls,
            segments: Some(segments.unwrap_or(crate::core::config::DEFAULT_SEGMENTS as u32)),
            limit_bps,
            use_clash: use_clash.unwrap_or(false),
        })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_bt_task(
    state: State<'_, Arc<AppState>>,
    source: String,
    save_dir: String,
) -> Result<String, String> {
    if source.is_empty() {
        return Err("磁力/种子源不能为空".into());
    }
    // 触发 BT 会话初始化（失败提前报错，而不是挂起任务后才失败）
    state.bt_engine().await?;
    state
        .manager
        .enqueue(NewTask {
            kind: TaskKind::Bt,
            name: source
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or("BT 任务")
                .trim_start_matches("magnet:")
                .to_string(),
            save_dir,
            category: String::new(),
            urls: vec![source],
            segments: None,
            limit_bps: None,
            use_clash: false,
        })
        .await
        .map_err(|e| e.to_string())
}

/// 重新下载：取消当前传输 → 清除断点残留（part/sidecar，保留已完成的最终文件）→
/// 进度归零 → 重新入队下载。任务 id 与创建时间保持不变。
#[tauri::command]
pub async fn restart_task(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    let record = state
        .store
        .get_task(&id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "任务不存在".to_string())?;
    // 运行中先取消并等待引擎退出（避免与旧任务的写盘竞争）
    state.manager.cancel(&id).await;
    for _ in 0..100 {
        let t = state.store.get_task(&id).await.map_err(|e| e.to_string())?;
        let settled = t
            .as_ref()
            .is_none_or(|t| !matches!(t.status, store::TaskStatus::Downloading));
        if settled {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    // HTTP：清分段残留，从零开始；BT 交由 librqbit 重新校验已有文件
    if record.kind == store::TaskKind::Http {
        clear_resume_artifacts(Path::new(&record.save_dir), &record.name);
    }
    state
        .store
        .reset_task_progress(&id)
        .await
        .map_err(|e| e.to_string())?;
    state.manager.resume(&id).await;
    Ok(())
}

/// 仅清理断点残留（{name}.part* / .vdl.json / .ghdl.json），不动已下载的最终文件。
fn clear_resume_artifacts(save_dir: &Path, name: &str) {
    let dir = Path::new(save_dir);
    let prefix_part = format!("{name}.part");
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let n = file_name.to_string_lossy();
            if n.starts_with(&prefix_part)
                || n == format!("{name}.vdl.json")
                || n == format!("{name}.ghdl.json")
            {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

/// 编辑任务（HTTP）：更新名称/保存目录/线程数/Clash 通道。
/// 运行中的任务会先取消，字段落库后按需自动续传（part 断点保留，改名/改目录/改线程数将重新开始）。
#[tauri::command]
pub async fn update_task(
    state: State<'_, Arc<AppState>>,
    id: String,
    name: String,
    save_dir: String,
    segments: Option<u32>,
    use_clash: bool,
) -> Result<(), String> {
    let record = state
        .store
        .get_task(&id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "任务不存在".to_string())?;
    if record.kind == store::TaskKind::Bt {
        return Err("BT 任务不支持编辑".into());
    }
    let name = name.trim();
    if name.is_empty() {
        return Err("文件名不能为空".into());
    }
    if save_dir.trim().is_empty() {
        return Err("保存目录不能为空".into());
    }
    let was_active = matches!(record.status, store::TaskStatus::Downloading);
    if was_active {
        state.manager.cancel(&id).await;
        for _ in 0..100 {
            let t = state.store.get_task(&id).await.map_err(|e| e.to_string())?;
            let settled = t
                .as_ref()
                .is_none_or(|t| !matches!(t.status, store::TaskStatus::Downloading));
            if settled {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
    state
        .store
        .update_task_fields(&id, name, save_dir.trim(), segments.map(|s| s as i64), use_clash)
        .await
        .map_err(|e| e.to_string())?;
    // 编辑前在下载的：用新参数自动续传（resume 冷恢复路径会重新入队）
    if was_active {
        state.manager.resume(&id).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn pause_task(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    state.manager.pause(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn resume_task(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    state.manager.resume(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn cancel_task(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    state.manager.cancel(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn remove_task(
    state: State<'_, Arc<AppState>>,
    id: String,
    delete_files: Option<bool>,
) -> Result<(), String> {
    // 运行中先取消；轮询等待引擎退出后再删行，避免竞态
    state.manager.cancel(&id).await;
    for _ in 0..100 {
        let t = state.store.get_task(&id).await.map_err(|e| e.to_string())?;
        let settled = t
            .as_ref()
            .is_none_or(|t| !matches!(t.status, store::TaskStatus::Downloading));
        if settled {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    // 可选：删除已下载的源文件与分段残留（BT 任务的目录结构不同，仅删记录）
    if delete_files.unwrap_or(false) {
        if let Some(t) = state.store.get_task(&id).await.map_err(|e| e.to_string())? {
            if t.kind == store::TaskKind::Http {
                delete_task_artifacts(Path::new(&t.save_dir), &t.name);
            }
        }
    }
    state.store.delete_task(&id).await.map_err(|e| e.to_string())
}

/// 删除任务的落地文件与分段残留（{name}.part* / .vdl.json / .ghdl.json）。
fn delete_task_artifacts(save_dir: &Path, name: &str) {
    let dir = Path::new(save_dir);
    let _ = std::fs::remove_file(dir.join(name));
    let prefix_part = format!("{name}.part");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let n = file_name.to_string_lossy();
        if n.starts_with(&prefix_part)
            || n == format!("{name}.vdl.json")
            || n == format!("{name}.ghdl.json")
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// 用系统默认程序打开任务文件；文件不存在时返回错误（前端回退打开目录）。
#[tauri::command]
pub async fn open_task_file(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    let t = state
        .store
        .get_task(&id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "任务不存在".to_string())?;
    let path = Path::new(&t.save_dir).join(&t.name);
    if !path.is_file() {
        return Err("文件尚未下载完成".into());
    }
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    Err("仅支持 Windows".into())
}

#[tauri::command]
pub async fn open_task_dir(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    let t = state
        .store
        .get_task(&id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "任务不存在".to_string())?;
    let dir = Path::new(&t.save_dir).join(&t.name);
    let dir = if dir.is_file() {
        dir.parent().map(Path::to_path_buf).unwrap_or(dir)
    } else {
        dir
    };
    #[cfg(windows)]
    std::process::Command::new("explorer")
        .arg(&dir)
        .spawn()
        .map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    {
        let _ = dir;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_settings(
    state: State<'_, Arc<AppState>>,
) -> Result<std::collections::HashMap<String, String>, String> {
    // 默认值打底 + 已存储覆盖（前端可直接渲染全部设置项）
    let mut out = crate::default_settings();
    for (k, v) in state.store.list_settings().await.map_err(|e| e.to_string())? {
        out.insert(k, v);
    }
    Ok(out)
}

#[tauri::command]
pub async fn set_setting(
    state: State<'_, Arc<AppState>>,
    key: String,
    value: String,
) -> Result<(), String> {
    state
        .store
        .set_setting(&key, &value)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn import_v2_config(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> Result<usize, String> {
    let cfg = store::import_read_config(Path::new(&path)).map_err(|e| e.to_string())?;
    store::migrate_v2_config(&state.store, &cfg)
        .await
        .map_err(|e| e.to_string())
}

/// 订阅任务事件流（引擎 Log/Progress/BtProgress/Finished）。
/// 每次调用注册一个前端接收通道；多次调用产生多份独立流。
#[tauri::command]
pub async fn subscribe_events(
    state: State<'_, Arc<AppState>>,
    on_event: tauri::ipc::Channel<(String, crate::engine::TaskEvent)>,
) -> Result<(), String> {
    let mut rx = state.manager.subscribe();
    tauri::async_runtime::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            if on_event.send(ev).is_err() {
                break; // 前端通道已关闭
            }
        }
    });
    Ok(())
}

/// Clash 链路测试结果：区分「TCP 端口不通」与「端口通但经代理的测试请求失败」。
#[derive(serde::Serialize)]
pub struct ClashTestResult {
    pub ok: bool,
    pub latency_ms: Option<u32>,
    /// TCP 端口是否可达（true + ok=false = 端口通但经代理的请求失败，多为节点/网络波动）
    pub tcp_open: bool,
}

/// 自动检测 Clash 端口（注册表系统代理优先 + 常见端口扫描，v2 同款算法）。
/// 同步命令在线程池执行，不阻塞主线程。
#[tauri::command]
pub fn detect_clash_port() -> Option<u16> {
    crate::core::proxy::detect_clash().map(|(port, _)| port)
}

/// 经指定端口实测 Clash 链路。
#[tauri::command]
pub fn test_clash_proxy(port: u16) -> ClashTestResult {
    if !crate::core::proxy::port_open(port, "127.0.0.1") {
        return ClashTestResult { ok: false, latency_ms: None, tcp_open: false };
    }
    match crate::core::proxy::ping_proxy(port, "127.0.0.1") {
        Some(ms) => ClashTestResult { ok: true, latency_ms: Some(ms), tcp_open: true },
        None => ClashTestResult { ok: false, latency_ms: None, tcp_open: true },
    }
}

/// 未使用的辅助占位：保持 save_path 语义集中（headless 也用它）。
#[allow(dead_code)]
pub(crate) fn save_path_of(save_dir: &str, name: &str) -> PathBuf {
    Path::new(save_dir).join(name)
}
