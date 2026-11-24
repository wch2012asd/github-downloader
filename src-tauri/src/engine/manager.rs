//! 任务管理器：队列、并发上限、自动补位。
//!
//! Store 状态写入口唯一化：任务引擎本身不落库，全部状态迁移由管理器负责，
//! 避免 SQLite 多写者竞态。事件经 broadcast 分发（C8 接 Tauri Channels）。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, mpsc};

use crate::core::config as cfg_defaults;
use crate::core::engines;
use crate::engine::bt::{self, BtEngine, BtSpec};
use crate::engine::downloader::{spawn_task, TaskControl, TaskEvent, TaskSpec};
use crate::engine::limiter::{Limiter, SpeedLimits};
use crate::store::{NewTask, Store, TaskKind, TaskStatus};

/// 活跃任务句柄。
struct ActiveTask {
    ctrl: TaskControl,
}

enum Cmd {
    /// 队列推进（已完成 store 插入）
    Push(String),
    Pause(String),
    Resume(String),
    Cancel(String),
    /// 引擎任务结束（转发线程解析 Finished 后携带结果发出）
    Done(String, bool, String),
    /// 进度落库（引擎事件 500ms 节流；HTTP/BT 共用）
    Progress(String, u64, Option<u64>),
}

pub struct TaskManager {
    store: Store,
    max_parallel: usize,
    inner: Arc<Mutex<Inner>>,
    cmd_tx: mpsc::UnboundedSender<Cmd>,
    events: broadcast::Sender<(String, TaskEvent)>,
    /// BT 会话（应用级共享，首次 BT 任务时按设置惰性创建）
    bt: tokio::sync::OnceCell<Arc<BtEngine>>,
}

#[derive(Default)]
struct Inner {
    /// 活跃任务（下载中或已暂停驻留引擎）
    active: HashMap<String, ActiveTask>,
    /// 等待启动的队列
    queue: Vec<String>,
    /// 暂停的排队任务
    paused: Vec<String>,
    /// 已请求取消的活跃任务（Done 时按此映射为 cancelled）
    cancel_requested: HashSet<String>,
}

impl TaskManager {
    /// 创建管理器并启动调度循环。`bt` 为共享 BT 会话槽（与上层共用，保证单一会话）。
    pub fn new(
        store: Store,
        max_parallel: usize,
        bt: tokio::sync::OnceCell<Arc<BtEngine>>,
    ) -> Arc<Self> {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (events, _) = broadcast::channel(1024);
        let mgr = Arc::new(Self {
            store,
            max_parallel: max_parallel.max(1),
            inner: Arc::new(Mutex::new(Inner::default())),
            cmd_tx,
            events,
            bt,
        });
        tokio::spawn(mgr_loop(mgr.clone(), cmd_rx));
        mgr
    }

    /// 订阅任务事件（任务 id, 事件）。
    pub fn subscribe(&self) -> broadcast::Receiver<(String, TaskEvent)> {
        self.events.subscribe()
    }

    /// 共享 BT 会话槽（供上层与 TaskManager 使用同一会话）。
    pub fn bt_slot(&self) -> tokio::sync::OnceCell<Arc<BtEngine>> {
        self.bt.clone()
    }

    /// 新建任务并入队；返回任务 id。入队前套用分类规则（C6：目录设置 + 类别标记）。
    pub async fn enqueue(&self, task: NewTask) -> sqlx::Result<String> {
        let task = self.apply_classification(task).await;
        let id = self.store.insert_task(&task).await?;
        let _ = self.cmd_tx.send(Cmd::Push(id.clone()));
        Ok(id)
    }

    /// 分类：按文件名扩展名标记类别；对应目录设置存在时改写保存目录。
    /// 总开关 `classify_enabled`（默认开）；调用方已填 category 时不覆盖。
    pub(crate) async fn apply_classification(&self, mut task: NewTask) -> NewTask {
        let enabled = self
            .setting_of("classify_enabled")
            .await
            .and_then(|s| s.parse::<bool>().ok())
            .unwrap_or(true);
        if !enabled || !task.category.is_empty() {
            return task;
        }
        if let Some(rule) = crate::core::classify::classify(&task.name) {
            task.category = rule.id.into();
            let dir_key = format!("dir_{}", rule.id);
            if let Some(dir) = self.setting_of(&dir_key).await.filter(|d| !d.is_empty()) {
                task.save_dir = dir;
            }
        }
        task
    }

    pub async fn pause(&self, id: &str) {
        let _ = self.cmd_tx.send(Cmd::Pause(id.to_string()));
    }

    pub async fn resume(&self, id: &str) {
        let _ = self.cmd_tx.send(Cmd::Resume(id.to_string()));
    }

    pub async fn cancel(&self, id: &str) {
        let _ = self.cmd_tx.send(Cmd::Cancel(id.to_string()));
    }

    /// 当前活跃任务数（含已暂停驻留的）。
    pub fn active_count(&self) -> usize {
        self.inner.lock().unwrap().active.len()
    }

    /// 读设置值（查询失败等同缺失）。
    async fn setting_of(&self, key: &str) -> Option<String> {
        self.store.get_setting(key).await.ok().flatten()
    }

    /// 用 Store 数据构造下载规格（通道变换/代理/超时按设置解析；无设置时默认直连）。
    async fn build_spec(&self, id: &str) -> Result<TaskSpec, String> {
        let record = self
            .store
            .get_task(id)
            .await
            .map_err(|e| format!("读取任务失败: {e}"))?
            .ok_or_else(|| "任务不存在".to_string())?;
        let sources = self.store.sources_of(id).await.map_err(|e| e.to_string())?;
        let url = sources
            .first()
            .map(|s| s.url.clone())
            .ok_or_else(|| "任务没有下载源".to_string())?;

        let setting = |k: &'static str| self.setting_of(k);
        let engine_id = setting("engine").await.unwrap_or_else(|| "direct".into());
        let ghproxy_prefix = setting("ghproxy_prefix")
            .await
            .unwrap_or_else(|| cfg_defaults::DEFAULT_GHPROXY_PREFIX.into());
        let kkgithub_host = setting("kkgithub_host")
            .await
            .unwrap_or_else(|| cfg_defaults::DEFAULT_KKGITHUB_HOST.into());
        let proxy_port = setting("proxy_port").await.and_then(|s| s.parse::<u16>().ok());
        let timeout_seconds = setting("timeout_seconds")
            .await
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(20);
        // 任务级 Clash 通道：勾选后本任务强制走 Clash 本地代理（URL 不套镜像前缀，对齐 v2 语义）
        let use_clash = record.use_clash;
        let transform = |u: &str| {
            if use_clash {
                return u.to_string();
            }
            if crate::core::url_parser::is_github_link(u) {
                engines::build_download_url(&engine_id, u, &ghproxy_prefix, &kkgithub_host)
            } else {
                u.to_string()
            }
        };

        let final_url = transform(&url);
        let mirrors = sources
            .iter()
            .skip(1)
            .map(|s| transform(&s.url))
            .collect::<Vec<_>>();
        let proxy = if use_clash {
            engines::proxies_for(
                "clash",
                Some(proxy_port.unwrap_or(cfg_defaults::DEFAULT_PROXY_PORT)),
            )
        } else {
            engines::proxies_for(&engine_id, proxy_port)
        };
        // 两层限速：全局（设置 global_limit_bps，0/缺失 = 不限）+ 任务级（任务记录覆盖）
        let global_limit = setting("global_limit_bps")
            .await
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|r| *r > 0)
            .map(|r| std::sync::Arc::new(Limiter::new(r)));
        let task_limit = record
            .limit_bps
            .map(|v| v as u64)
            .filter(|r| *r > 0)
            .map(|r| std::sync::Arc::new(Limiter::new(r)));
        Ok(TaskSpec {
            url: final_url,
            mirrors,
            alt_url: None,
            save_path: save_path_of(&record.save_dir, &record.name),
            proxy,
            segments: record
                .segments
                .unwrap_or(cfg_defaults::DEFAULT_SEGMENTS as u32) as u8,
            timeout_seconds,
            limits: SpeedLimits { global: global_limit, task: task_limit },
        })
    }

    /// 按设置惰性创建共享 BT 会话。
    async fn bt_engine(&self) -> Result<Arc<BtEngine>, String> {
        if let Some(e) = self.bt.get() {
            return Ok(e.clone());
        }
        let default_dir = self
            .setting_of("save_dir")
            .await
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| ".".into());
        let port = self.setting_of("bt_listen_port").await.and_then(|s| s.parse::<u16>().ok());
        let enable_dht = self
            .setting_of("bt_enable_dht")
            .await
            .and_then(|s| s.parse::<bool>().ok())
            .unwrap_or(true);
        let enable_upnp = self
            .setting_of("bt_enable_upnp")
            .await
            .and_then(|s| s.parse::<bool>().ok())
            .unwrap_or(true);
        let engine = BtEngine::new(PathBuf::from(default_dir), port, enable_dht, enable_upnp)
            .await
            .map_err(|e| format!("BT 会话创建失败: {e}"))?;
        Ok(self.bt.get_or_init(move || async move { engine }).await.clone())
    }

    /// 启动一个任务；返回其控制句柄（调用方负责注册活跃表）。规格不可用时返回 Err。
    async fn spawn_one(&self, id: &str) -> Result<TaskControl, String> {
        let record = self
            .store
            .get_task(id)
            .await
            .map_err(|e| format!("读取任务失败: {e}"))?
            .ok_or_else(|| "任务不存在".to_string())?;
        // 保存目录不存在时创建（新建任务/分类目录都可能指向未创建的路径）
        if let Err(e) = std::fs::create_dir_all(&record.save_dir) {
            return Err(format!("无法创建保存目录 {}: {e}", record.save_dir));
        }
        let ctrl = TaskControl::new();
        let (tx, rx) = mpsc::unbounded_channel::<TaskEvent>();
        match record.kind {
            TaskKind::Http => {
                let spec = self.build_spec(id).await?;
                spawn_task(spec, ctrl.clone(), tx);
            }
            TaskKind::Bt => {
                let engine = self.bt_engine().await?;
                let source = self
                    .store
                    .sources_of(id)
                    .await
                    .map_err(|e| e.to_string())?
                    .first()
                    .map(|s| s.url.clone())
                    .ok_or_else(|| "BT 任务缺少磁力/种子源".to_string())?;
                let spec = BtSpec {
                    source,
                    save_dir: record.save_dir.clone(),
                    initial_peers: Vec::new(),
                };
                // 包装：bt::run 的任何错误路径都必须以 Finished 收尾（否则任务卡死在下载中）
                let run_ctrl = ctrl.clone();
                tokio::spawn(async move {
                    let result = bt::run(engine, spec, run_ctrl, tx.clone()).await;
                    let reason = match &result {
                        Ok(()) => String::new(),
                        Err(crate::engine::downloader::TaskFail::Cancelled) => "已取消".into(),
                        Err(crate::engine::downloader::TaskFail::Failed(r)) => r.clone(),
                    };
                    let _ = tx.send(TaskEvent::Finished { ok: result.is_ok(), reason });
                });
            }
        }
        // 事件转发：广播给订阅者；解析 Finished 后通知管理器收尾
        let events = self.events.clone();
        let cmd_tx = self.cmd_tx.clone();
        let tid = id.to_string();
        tokio::spawn(forward_events(tid, rx, events, cmd_tx));
        Ok(ctrl)
    }
}

/// 保存路径 = save_dir + name。
fn save_path_of(save_dir: &str, name: &str) -> PathBuf {
    Path::new(save_dir).join(name)
}

async fn mgr_loop(mgr: Arc<TaskManager>, mut rx: mpsc::UnboundedReceiver<Cmd>) {
    while let Some(cmd) = rx.recv().await {
        mgr.apply(cmd).await;
    }
}

/// 事件转发：广播给订阅者；Progress 转发引擎 500ms 节流直接落库；Finished 通知管理器收尾。
async fn forward_events(
    id: String,
    mut rx: mpsc::UnboundedReceiver<TaskEvent>,
    events: broadcast::Sender<(String, TaskEvent)>,
    cmd_tx: mpsc::UnboundedSender<Cmd>,
) {
    let mut outcome = (false, String::new());
    while let Some(ev) = rx.recv().await {
        match ev {
            TaskEvent::Finished { ok, reason } => {
                outcome = (ok, reason.clone());
                let _ = events.send((id.clone(), TaskEvent::Finished { ok, reason }));
                break;
            }
            TaskEvent::Progress(p) => {
                let _ = events.send((id.clone(), TaskEvent::Progress(p)));
                cmd_tx.send(Cmd::Progress(id.clone(), p.downloaded, Some(p.total))).ok();
            }
            TaskEvent::BtProgress(p) => {
                let _ = events.send((id.clone(), TaskEvent::BtProgress(p.clone())));
                cmd_tx.send(Cmd::Progress(id.clone(), p.downloaded, Some(p.total))).ok();
            }
            other => {
                let _ = events.send((id.clone(), other));
            }
        }
    }
    cmd_tx.send(Cmd::Done(id, outcome.0, outcome.1)).ok();
}

impl TaskManager {
    async fn apply(&self, cmd: Cmd) {
        match cmd {
            Cmd::Push(id) => {
                self.inner.lock().unwrap().queue.push(id);
                self.fill().await;
            }
            Cmd::Pause(id) => {
                // 锁内只做决策与控制，锁外落库（MutexGuard 不得跨 await）
                let should_pause = {
                    let mut inner = self.inner.lock().unwrap();
                    if let Some(t) = inner.active.get(&id) {
                        t.ctrl.pause();
                        true
                    } else if let Some(pos) = inner.queue.iter().position(|q| q == &id) {
                        inner.queue.remove(pos);
                        inner.paused.push(id.clone());
                        true
                    } else {
                        false
                    }
                };
                if should_pause {
                    self.set_status(&id, TaskStatus::Paused, None).await;
                }
            }
            Cmd::Resume(id) => {
                enum Outcome {
                    /// 活跃任务恢复 → Downloading
                    Active,
                    /// 排队任务恢复 → 重新入队并补位
                    Requeued,
                    /// 内存不认识但库里可续（典型：应用重启后的 paused 任务）→ None 分支内冷恢复
                    None,
                }
                let outcome = {
                    let mut inner = self.inner.lock().unwrap();
                    if let Some(t) = inner.active.get(&id) {
                        t.ctrl.resume();
                        Outcome::Active
                    } else if let Some(pos) = inner.paused.iter().position(|q| q == &id) {
                        let id2 = inner.paused.remove(pos);
                        inner.queue.push(id2);
                        Outcome::Requeued
                    } else if inner.queue.iter().any(|q| q == &id) {
                        Outcome::Requeued
                    } else {
                        Outcome::None
                    }
                };
                match outcome {
                    Outcome::Active => {
                        self.set_status(&id, TaskStatus::Downloading, None).await;
                    }
                    Outcome::Requeued => {
                        self.set_status(&id, TaskStatus::Queued, None).await;
                        self.fill().await;
                    }
                    Outcome::None => {
                        // 冷恢复：应用重启后任务只在 SQLite，内存队列为空。
                        // 库里处于可续状态则重新入队，由 fill 派给引擎做断点续传。
                        let resumable = self
                            .store
                            .get_task(&id)
                            .await
                            .ok()
                            .flatten()
                            .is_some_and(|t| {
                                matches!(
                                    t.status,
                                    TaskStatus::Paused
                                        | TaskStatus::Queued
                                        | TaskStatus::Failed
                                        | TaskStatus::Cancelled
                                )
                            });
                        if resumable {
                            self.inner.lock().unwrap().queue.push(id.clone());
                            self.set_status(&id, TaskStatus::Queued, None).await;
                            self.fill().await;
                        }
                    }
                }
            }
            Cmd::Cancel(id) => {
                let removed_from_list = {
                    let mut inner = self.inner.lock().unwrap();
                    // 先克隆 ctrl 再改 inner，避免同时持有可变与不可变借用
                    let ctrl = inner.active.get(&id).map(|t| t.ctrl.clone());
                    if let Some(t) = ctrl {
                        inner.cancel_requested.insert(id.clone());
                        t.stop();
                        false
                    } else if let Some(pos) = inner.queue.iter().position(|q| q == &id) {
                        inner.queue.remove(pos);
                        true
                    } else if let Some(pos) = inner.paused.iter().position(|q| q == &id) {
                        inner.paused.remove(pos);
                        true
                    } else {
                        false
                    }
                };
                if removed_from_list {
                    self.set_status(&id, TaskStatus::Cancelled, None).await;
                }
            }
            Cmd::Progress(id, received, total) => {
                let _ = self
                    .store
                    .update_progress(&id, received as i64, total.map(|t| t as i64))
                    .await;
            }
            Cmd::Done(id, ok, reason) => {
                let was_cancel = {
                    let mut inner = self.inner.lock().unwrap();
                    inner.active.remove(&id);
                    inner.cancel_requested.remove(&id)
                };
                if was_cancel {
                    self.set_status(&id, TaskStatus::Cancelled, None).await;
                } else if ok {
                    self.set_status(&id, TaskStatus::Completed, None).await;
                } else {
                    let err = if reason.is_empty() { "下载失败".to_string() } else { reason };
                    self.set_status(&id, TaskStatus::Failed, Some(&err)).await;
                }
                self.fill().await;
            }
        }
    }

    /// 状态落库 + 广播（前端状态栏/仪表台的唯一事实来源）。
    async fn set_status(&self, id: &str, status: TaskStatus, error: Option<&str>) {
        let _ = self.store.update_status(id, status, error).await;
        let _ = self
            .events
            .send((id.to_string(), TaskEvent::Status(status.as_str().to_string())));
    }

    /// 由外部通道（浏览器 blob 接管）直接落盘的任务：置完成态并广播事件。
    pub async fn mark_completed(&self, id: &str) {
        self.set_status(id, TaskStatus::Completed, None).await;
    }

    /// 并行空闲位填满队列。
    async fn fill(&self) {
        loop {
            let (can_start, next) = {
                let inner = self.inner.lock().unwrap();
                (
                    inner.active.len() < self.max_parallel,
                    inner.queue.first().cloned(),
                )
            };
            let Some(id) = next else { return };
            if !can_start {
                return;
            }
            match self.spawn_one(&id).await {
                Ok(ctrl) => {
                    {
                        let mut inner = self.inner.lock().unwrap();
                        inner.queue.remove(0);
                        inner.active.insert(id.clone(), ActiveTask { ctrl });
                    }
                    self.set_status(&id, TaskStatus::Downloading, None).await;
                }
                Err(e) => {
                    // 规格不可用（无源等）：跳过并标记失败，继续补位
                    {
                        let mut inner = self.inner.lock().unwrap();
                        inner.queue.remove(0);
                    }
                    self.set_status(&id, TaskStatus::Failed, Some(&e)).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::TaskKind;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::time::Duration;

    /// 极简假服务器：计数并发请求数（含 HEAD），可选延迟后返回完整内容。
    async fn serve_counting(
        n_requests: usize,
        body: Vec<u8>,
        ranges: bool,
        delay_ms: u64,
        max_seen: Arc<AtomicUsize>,
    ) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let inflight = Arc::new(AtomicUsize::new(0));
            for _ in 0..n_requests {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let max = max_seen.clone();
                let inflight = inflight.clone();
                let body = body.clone();
                tokio::spawn(async move {
                    let cur = inflight.fetch_add(1, AtomicOrdering::SeqCst) + 1;
                    max.fetch_max(cur, AtomicOrdering::SeqCst);
                    let (mut r, mut w) = stream.into_split();
                    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
                    let mut reader = tokio::io::BufReader::new(&mut r);
                    let mut line = String::new();
                    let _ = reader.read_line(&mut line).await;
                    loop {
                        let mut h = String::new();
                        let n = reader.read_line(&mut h).await.unwrap_or(0);
                        if n == 0 || h.trim_end().is_empty() {
                            break;
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    let total = body.len();
                    let mut head = format!("HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {total}\r\n");
                    if ranges {
                        head.push_str("Accept-Ranges: bytes\r\n");
                    }
                    head.push_str("\r\n");
                    let _ = w.write_all(head.as_bytes()).await;
                    let _ = w.write_all(&body).await;
                    let _ = w.flush().await;
                    inflight.fetch_sub(1, AtomicOrdering::SeqCst);
                });
            }
        });
        format!("http://{addr}/file.bin")
    }

    fn task_of(name: &str, url: String, dir: &Path) -> NewTask {
        NewTask {
            kind: TaskKind::Http,
            name: name.into(),
            save_dir: dir.to_string_lossy().into_owned(),
            category: String::new(),
            urls: vec![url],
            segments: Some(1),
            limit_bps: None,
            use_clash: false,
        }
    }

    fn mgr(store: Store, parallel: usize) -> Arc<TaskManager> {
        TaskManager::new(store, parallel, tokio::sync::OnceCell::new())
    }

    fn work_dir(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("vdl-mgr-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    async fn wait_for(
        store: &Store,
        pred: impl Fn(&[crate::store::TaskRecord]) -> bool,
        secs: u64,
    ) -> Vec<crate::store::TaskRecord> {
        let deadline = std::time::Instant::now() + Duration::from_secs(secs);
        loop {
            let list = store.list_tasks().await.unwrap();
            if pred(&list) {
                return list;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "等待任务状态超时: {:?}",
                list.iter().map(|t| (t.name.as_str(), t.status)).collect::<Vec<_>>()
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    #[tokio::test]
    async fn respects_parallel_limit_and_autofills() {
        let store = Store::open_memory().await.unwrap();
        let max_seen = Arc::new(AtomicUsize::new(0));
        let body = (0..256 * 1024).map(|i| (i % 251) as u8).collect::<Vec<_>>();
        // 延迟 80ms 保证请求重叠可被观测
        let url = serve_counting(8, body.clone(), false, 80, max_seen.clone()).await;
        let dir = work_dir("limit");
        let mgr = mgr(store.clone(), 2);

        let mut ids = Vec::new();
        for i in 0..4 {
            let id = mgr.enqueue(task_of(&format!("f{i}.bin"), url.clone(), &dir)).await.unwrap();
            ids.push(id);
        }
        let list = wait_for(&store, |l| l.iter().all(|t| t.status == TaskStatus::Completed), 60).await;
        assert_eq!(list.len(), 4, "四个任务都应完成并自动补位");
        // 同一时刻最多 2 个活跃任务 → 服务器并发请求 ≤ 2
        assert!(
            max_seen.load(AtomicOrdering::SeqCst) <= 2,
            "并发请求 {} 超过并行上限 2",
            max_seen.load(AtomicOrdering::SeqCst)
        );
        // 文件内容正确
        for (i, id) in ids.iter().enumerate() {
            let t = store.get_task(id).await.unwrap().unwrap();
            let got = std::fs::read(save_path_of(&t.save_dir, &t.name)).unwrap();
            assert_eq!(got, body, "任务 {i} 内容一致");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 回归：应用重启后任务只在 SQLite（内存队列为空），点恢复必须冷启动续传。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn cold_resume_after_restart_starts_task() {
        let store = Store::open_memory().await.unwrap();
        let body = vec![5u8; 64 * 1024];
        let url = serve_counting(2, body.clone(), false, 5, Arc::new(AtomicUsize::new(0))).await;
        let dir = work_dir("cold-resume");
        let id = store
            .insert_task(&NewTask {
                kind: TaskKind::Http,
                name: "cold.bin".into(),
                save_dir: dir.to_string_lossy().into_owned(),
                category: String::new(),
                urls: vec![url],
                segments: Some(1),
                limit_bps: None,
                use_clash: false,
            })
            .await
            .unwrap();
        // 模拟重启现场：库里 paused，新 manager 内存全空
        store.update_status(&id, TaskStatus::Paused, None).await.unwrap();
        let mgr = mgr(store.clone(), 1);
        mgr.resume(&id).await;
        wait_for(&store, |l| {
            l.iter().all(|t| t.status == TaskStatus::Completed)
        }, 30)
        .await;
        assert_eq!(
            std::fs::read(save_path_of(&dir.to_string_lossy(), "cold.bin")).unwrap(),
            body
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn queued_pause_resume_and_cancel_active() {
        let store = Store::open_memory().await.unwrap();
        // 慢服务器给任务 A（取消窗口）；快服务器给任务 B
        let slow_body = vec![7u8; 512 * 1024];
        let url_a = serve_counting(10, slow_body, false, 300, Arc::new(AtomicUsize::new(0))).await;
        let fast_body = vec![9u8; 64 * 1024];
        let url_b = serve_counting(5, fast_body, false, 10, Arc::new(AtomicUsize::new(0))).await;
        let dir = work_dir("lifecycle");
        let mgr = mgr(store.clone(), 1);

        let a = mgr.enqueue(task_of("a.bin", url_a, &dir)).await.unwrap();
        let b = mgr.enqueue(task_of("b.bin", url_b, &dir)).await.unwrap();

        // B 仍在排队时暂停
        mgr.pause(&b).await;
        wait_for(&store, |l| {
            l.iter().all(|t| match t.name.as_str() {
                "b.bin" => t.status == TaskStatus::Paused,
                _ => matches!(t.status, TaskStatus::Downloading | TaskStatus::Completed),
            })
        }, 30)
        .await;

        // 取消运行中的 A
        mgr.cancel(&a).await;
        let list = wait_for(&store, |l| {
            l.iter().all(|t| match t.name.as_str() {
                "a.bin" => t.status == TaskStatus::Cancelled,
                "b.bin" => t.status == TaskStatus::Paused, // 队列已空，暂停任务不得自动启动
                _ => true,
            })
        }, 30)
        .await;
        assert!(mgr.active_count() == 0 || list.iter().all(|t| t.name != "a.bin"));

        // 恢复 B → 唯一空闲位 → 完成
        mgr.resume(&b).await;
        wait_for(&store, |l| {
            l.iter().all(|t| match t.name.as_str() {
                "b.bin" => t.status == TaskStatus::Completed,
                _ => t.status == TaskStatus::Cancelled,
            })
        }, 30)
        .await;
        let _ = std::fs::remove_dir_all(&dir);
    }
}
