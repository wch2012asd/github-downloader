//! BT 引擎封装（librqbit）：磁力/.torrent 任务接入统一任务模型。
//!
//! - 应用级共享一个 [`BtEngine`]（一个监听端口、一份 DHT 状态），多任务并存
//! - 进度经 [`TaskEvent::BtProgress`] 上报：↓/↑ 速率、DHT 节点数、百分比
//! - 默认完成即停做种（`session.pause` 停止上传）；分享比例上限留给设置层
//! - 暂停/取消：`ctrl` 状态映射到 session.pause/unpause

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use librqbit::{AddTorrent, AddTorrentOptions, ListenerOptions, Session, SessionOptions};
use tokio::sync::mpsc;

use crate::engine::downloader::{LogLevel, TaskControl, TaskEvent, TaskFail};

/// BT 任务输入。
pub struct BtSpec {
    /// 磁力链接或 .torrent 文件路径
    pub source: String,
    pub save_dir: String,
    /// 注入的初始 peer（测试直连；生产走 DHT/Tracker）
    pub initial_peers: Vec<SocketAddr>,
}

/// BT 进度快照（500ms 节流）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct BtProgress {
    pub downloaded: u64,
    pub total: u64,
    pub down_bps: u64,
    pub up_bps: u64,
    pub pct: f64,
    /// DHT 路由表节点数（DHT 未启用 = 0）
    pub dht_nodes: u32,
}

/// BT 会话封装（应用级共享）。
pub struct BtEngine {
    session: Arc<Session>,
}

impl BtEngine {
    /// 创建 BT 会话。
    ///
    /// `listen_port = None` 时由 librqbit 使用默认（随机端口）；
    /// `enable_upnp` 控制 UPnP 端口映射尝试。
    pub async fn new(
        default_dir: PathBuf,
        listen_port: Option<u16>,
        enable_dht: bool,
        enable_upnp: bool,
    ) -> anyhow::Result<Arc<Self>> {
        let opts = SessionOptions {
            dht: if enable_dht {
                Some(librqbit::DhtSessionConfig::default())
            } else {
                None
            },
            listen: Some(ListenerOptions {
                listen_addr: format!("0.0.0.0:{}", listen_port.unwrap_or(0))
                    .parse()
                    .map_err(|e| anyhow::anyhow!("BT 监听地址解析失败: {e}"))?,
                enable_upnp_port_forwarding: enable_upnp,
                ..Default::default()
            }),
            ..Default::default()
        };
        let session = Session::new_with_opts(default_dir, opts).await?;
        Ok(Arc::new(Self { session }))
    }

    /// 从磁力/种子路径添加任务并返回句柄；重复添加返回已存在句柄。
    pub async fn add(
        &self,
        source: &str,
        save_dir: &str,
        initial_peers: Vec<SocketAddr>,
    ) -> Result<std::sync::Arc<librqbit::ManagedTorrent>, TaskFail> {
        let mut opts = AddTorrentOptions {
            output_folder: Some(save_dir.to_string()),
            overwrite: true,
            ..Default::default()
        };
        if !initial_peers.is_empty() {
            opts.initial_peers = Some(initial_peers);
        }
        let add = AddTorrent::from_cli_argument(source)
            .map_err(|e| TaskFail::Failed(format!("BT 源解析失败: {e}")))?;
        let resp = self
            .session
            .add_torrent(add, Some(opts))
            .await
            .map_err(|e| TaskFail::Failed(format!("BT 任务添加失败: {e}")))?;
        resp.into_handle()
            .ok_or_else(|| TaskFail::Failed("BT 任务返回无效句柄".into()))
    }
}

/// 执行一个 BT 任务；完成即停做种。
pub async fn run(
    engine: Arc<BtEngine>,
    spec: BtSpec,
    ctrl: TaskControl,
    tx: mpsc::UnboundedSender<TaskEvent>,
) -> Result<(), TaskFail> {
    let handle = engine.add(&spec.source, &spec.save_dir, spec.initial_peers).await?;
    // 元数据等待（磁力链接需要先从 peer 拉取 metadata；.torrent 立即返回）
    handle
        .wait_until_initialized()
        .await
        .map_err(|e| TaskFail::Failed(format!("BT 元数据获取失败: {e}")))?;

    log_evt(&tx, LogLevel::Info, format!("BT 任务开始: {}", handle.name().unwrap_or_default()));
    let mut session_paused = false;
    loop {
        if !ctrl.wait_running().await {
            let _ = engine.session.pause(&handle).await;
            log_evt(&tx, LogLevel::Warn, "下载已取消".into());
            return Err(TaskFail::Cancelled);
        }
        // 暂停/恢复映射到 librqbit（暂停同时停上传）
        match ctrl.state() {
            crate::engine::downloader::Ctrl::Paused if !session_paused => {
                let _ = engine.session.pause(&handle).await;
                session_paused = true;
            }
            crate::engine::downloader::Ctrl::Running if session_paused => {
                let _ = engine.session.unpause(&handle).await;
                session_paused = false;
            }
            _ => {}
        }

        let st = handle.stats();
        let (down_bps, up_bps) = st
            .live
            .as_ref()
            .map(|l| (l.download_speed.as_bytes(), l.upload_speed.as_bytes()))
            .unwrap_or((0, 0));
        let dht_nodes = engine
            .session
            .get_dht()
            .map(|d| d.get_stats().routing_table_size as u32)
            .unwrap_or(0);
        let pct = if st.total_bytes > 0 {
            st.progress_bytes as f64 / st.total_bytes as f64 * 100.0
        } else {
            0.0
        };
        let _ = tx.send(TaskEvent::BtProgress(BtProgress {
            downloaded: st.progress_bytes,
            total: st.total_bytes,
            down_bps,
            up_bps,
            pct,
            dht_nodes,
        }));

        if st.finished {
            // 默认完成即停做种：pause 同时停止上传
            let _ = engine.session.pause(&handle).await;
            let _ = tx.send(TaskEvent::Progress(crate::engine::downloader::Progress {
                downloaded: st.progress_bytes,
                total: st.total_bytes.max(st.progress_bytes),
                speed_bps: 0.0,
                pct: 100.0,
            }));
            log_evt(
                &tx,
                LogLevel::Success,
                format!("下载完成: {} ({} 字节)", spec.save_dir, st.progress_bytes),
            );
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

fn log_evt(tx: &mpsc::UnboundedSender<TaskEvent>, level: LogLevel, msg: String) {
    let _ = tx.send(TaskEvent::Log(level, msg));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    const SEED_PORT: u16 = 46123;
    const LEECH_PORT: u16 = 46124;

    async fn make_engine(port: u16, dir: PathBuf) -> Arc<BtEngine> {
        BtEngine::new(dir, Some(port), false, false).await.unwrap()
    }

    async fn wait_finished(
        rx: &mut mpsc::UnboundedReceiver<TaskEvent>,
        secs: u64,
    ) -> (bool, String, Vec<TaskEvent>) {
        let mut events = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(secs);
        loop {
            assert!(std::time::Instant::now() < deadline, "BT 测试超时");
            match tokio::time::timeout(Duration::from_secs(1), rx.recv()).await {
                Ok(Some(e)) => {
                    let done = matches!(&e, TaskEvent::Finished { .. });
                    events.push(e);
                    if done {
                        break;
                    }
                }
                Ok(None) => panic!("事件通道关闭"),
                Err(_) => continue,
            }
        }
        match events.last() {
            Some(TaskEvent::Finished { ok, reason, .. }) => (*ok, reason.clone(), events),
            _ => panic!("缺少 Finished 事件"),
        }
    }

    /// 离线端到端：本机种子（librqbit create_torrent）→ 磁力 leech（initial_peers 直连）。
    /// 验证：元数据获取、下载完成、文件一致、进度事件流。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn it_bt_seed_to_leech_via_magnet() {
        let tmp = std::env::temp_dir().join(format!("vdl-bt-{}-seed", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let payload_name = "payload.bin";
        let payload: Vec<u8> = (0..512 * 1024u32).map(|i| (i * 7 % 251) as u8).collect();
        let payload_path = tmp.join(payload_name);
        std::fs::write(&payload_path, &payload).unwrap();

        // 造种子（librqbit create_torrent）
        let spawner = librqbit::spawn_utils::BlockingSpawner::new(4);
        let created = librqbit::create_torrent(
            &payload_path,
            librqbit::CreateTorrentOptions {
                name: Some(payload_name),
                trackers: vec![],
                piece_length: Some(16 * 1024),
            },
            &spawner,
        )
        .await
        .unwrap();
        let torrent_file = tmp.join("payload.torrent");
        std::fs::write(&torrent_file, created.as_bytes().unwrap()).unwrap();
        let magnet = format!(
            "magnet:?xt=urn:btih:{}&dn={payload_name}",
            created.info_hash().as_string()
        );

        // 种子会话：挂载输出目录中的文件做种
        let seed_engine = make_engine(SEED_PORT, tmp.clone()).await;
        seed_engine
            .add(&torrent_file.to_string_lossy(), &tmp.to_string_lossy(), vec![])
            .await
            .unwrap();

        // leech 会话：磁力 + 注入种子 peer
        let leech_dir = std::env::temp_dir().join(format!("vdl-bt-{}-leech", std::process::id()));
        let _ = std::fs::remove_dir_all(&leech_dir);
        std::fs::create_dir_all(&leech_dir).unwrap();
        let leech_engine = make_engine(LEECH_PORT, leech_dir.clone()).await;
        let ctrl = TaskControl::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let spec = BtSpec {
            source: magnet,
            save_dir: leech_dir.to_string_lossy().into_owned(),
            initial_peers: vec![SocketAddr::from((Ipv4Addr::LOCALHOST, SEED_PORT))],
        };
        let engine = leech_engine.clone();
        let handle = tokio::spawn(async move {
            let result = crate::engine::bt::run(engine, spec, ctrl.clone(), tx.clone()).await;
            let reason = match &result {
                Ok(()) => String::new(),
                Err(TaskFail::Cancelled) => "已取消".into(),
                Err(TaskFail::Failed(r)) => r.clone(),
            };
            let _ = tx.send(TaskEvent::Finished { ok: result.is_ok(), reason });
        });

        let (ok, reason, events) = wait_finished(&mut rx, 120).await;
        handle.abort();
        assert!(ok, "BT 下载失败: {reason}");
        assert!(
            events.iter().any(|e| matches!(e,
                TaskEvent::BtProgress(p) if p.down_bps > 0 || p.pct >= 100.0
            )),
            "应有 BtProgress 事件"
        );
        let got = std::fs::read(leech_dir.join(payload_name)).unwrap();
        assert_eq!(got, payload, "BT 下载内容应与源一致");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&leech_dir);
    }
}
