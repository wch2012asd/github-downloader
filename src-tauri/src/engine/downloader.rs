//! 下载核心（async 版）：探测 → 规划 → 单流/分段下载 → 合并，断点续传与自动重试。
//!
//! 语义对齐 v2 downloader.rs（作为移植规范）：
//! - 重试：最多 8 次尝试、间隔 5s、仅网络错误与 5xx 可重试、416 判完整、200 降级重下
//! - 探测：HEAD 优先，405/403 等拒绝时退化 `GET Range: bytes=0-0`
//! - 分段：`accept_ranges && total ≥ 8MiB && n > 1`，每段 ≥1MiB，part 文件长度即进度
//! - 读超时：两次数据之间最多等 30s（v2 的 StallReader 线程语义 → `tokio::time::timeout` 包裹 chunk 读），
//!   超时按可重试中断处理，已写入数据保留
//!
//! 与 v2 的差异：实现层 tokio + async reqwest；分段线程组为 `JoinSet`；
//! 控制通道 watch（替代 Condvar）；sidecar 更名 `.vdl.json`（兼容读取 v2 `.ghdl.json`）。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;

use crate::core::url_parser::percent_decode;
use crate::engine::limiter::SpeedLimits;

const MAX_ATTEMPTS: u32 = 8; // 首次下载 + 最多 7 次自动重试（对齐 v1）
const RETRY_DELAY: Duration = Duration::from_secs(5);
const TICK: Duration = Duration::from_millis(500);
/// 两次数据之间最多等待的时长（v1 的 read timeout 语义）；超时按可重试中断处理
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// 总大小低于该阈值时退回单流（小文件分段得不偿失）
const SEGMENT_MIN_SIZE: u64 = 8 * 1024 * 1024;
/// 每段最小跨度，避免碎片段
const SEGMENT_MIN_SPAN: u64 = 1024 * 1024;
const MAX_SEGMENTS: u8 = 32;

/// 日志级别（v2 events.rs 对齐；Proxy 级保留给通道切换类事件）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum LogLevel {
    Info,
    Success,
    Warn,
    Error,
    Proxy,
}

/// 进度快照（500ms 节流由下载循环保证）。
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Progress {
    pub downloaded: u64,
    pub total: u64,
    pub speed_bps: f64,
    pub pct: f64,
}

/// 任务事件：引擎 → 上层（管理器/界面）。
#[derive(Debug, Clone, serde::Serialize)]
pub enum TaskEvent {
    Log(LogLevel, String),
    Progress(Progress),
    /// BT 任务进度（↓/↑ 速率、DHT 节点数；500ms 节流）
    BtProgress(crate::engine::bt::BtProgress),
    /// 状态迁移广播（管理器在每次落库时同步发出）
    Status(String),
    Finished { ok: bool, reason: String },
}

/// 任务控制状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ctrl {
    Running,
    Paused,
    Stopping,
}

/// 跨任务共享的暂停/恢复/停止控制（watch 通道；chunk 间隙轮询生效）。
#[derive(Clone)]
pub struct TaskControl(Arc<(watch::Sender<Ctrl>, watch::Receiver<Ctrl>)>);

impl Default for TaskControl {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskControl {
    pub fn new() -> Self {
        Self(Arc::new(watch::channel(Ctrl::Running)))
    }

    fn set(&self, c: Ctrl) {
        let _ = self.0 .0.send(c);
    }

    pub fn pause(&self) {
        self.set(Ctrl::Paused);
    }

    pub fn resume(&self) {
        self.set(Ctrl::Running);
    }

    pub fn stop(&self) {
        self.set(Ctrl::Stopping);
    }

    pub fn state(&self) -> Ctrl {
        *self.0 .1.borrow()
    }

    /// 等待任务恢复运行；返回 false 表示已请求停止。
    /// borrow_and_update + changed 无唤醒丢失竞态（v2 Condvar 语义的等价实现）。
    pub async fn wait_running(&self) -> bool {
        let mut rx = self.0 .1.clone();
        loop {
            // 先取出状态（结束 RefGuard 临时借用）再决定等待，避免借用冲突
            let state = *rx.borrow_and_update();
            match state {
                Ctrl::Running => return true,
                Ctrl::Stopping => return false,
                Ctrl::Paused => {
                    if rx.changed().await.is_err() {
                        return false;
                    }
                }
            }
        }
    }
}

/// 下载前探测到的资源元数据。
#[derive(Debug, Clone, Default)]
pub struct Probe {
    pub total: Option<u64>,
    pub accept_ranges: bool,
    pub etag: Option<String>,
    pub filename: Option<String>,
    /// 响应 Content-Type（用于提示"链接返回的是网页而非文件"）
    pub content_type: Option<String>,
}

/// 站点许可门槛表单（如 SEGGER：需 POST 确认接受条款后才返回文件）。
#[derive(Debug, Clone)]
pub struct GateForm {
    /// 表单提交地址（绝对 URL）
    pub action: String,
    /// urlencoded 的表单字段
    pub body: String,
}

/// 一个下载任务的输入。`url` 是按通道变换后的最终地址。
pub struct TaskSpec {
    pub url: String,
    /// 额外镜像源（同一文件的不同 URL，v3 多源聚合）；经同样通道变换
    pub mirrors: Vec<String>,
    /// 主地址 404 时的备用地址（仓库主页 main.zip → master.zip 兜底）
    pub alt_url: Option<String>,
    pub save_path: PathBuf,
    /// Some("http://127.0.0.1:port")，仅 clash 通道
    pub proxy: Option<String>,
    pub segments: u8,
    /// 连接与探测超时（秒），来自设置的「网络连接请求超时」
    pub timeout_seconds: u32,
    /// 两层限速（全局 + 任务级）；默认不限速
    pub limits: SpeedLimits,
}

#[derive(Debug)]
pub enum TaskFail {
    Cancelled,
    Failed(String),
}

/// 启动下载任务；结束时发送 [`TaskEvent::Finished`]。
pub fn spawn_task(
    spec: TaskSpec,
    ctrl: TaskControl,
    tx: mpsc::UnboundedSender<TaskEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let result = run(&spec, &ctrl, &tx).await;
        let reason = match &result {
            Ok(()) => String::new(),
            Err(TaskFail::Cancelled) => "已取消".into(),
            Err(TaskFail::Failed(r)) => r.clone(),
        };
        let _ = tx.send(TaskEvent::Finished { ok: result.is_ok(), reason });
    })
}

fn log_evt(tx: &mpsc::UnboundedSender<TaskEvent>, level: LogLevel, msg: String) {
    let _ = tx.send(TaskEvent::Log(level, msg));
}

/// 执行一次下载任务（编排入口）；暂停/取消语义见 [`TaskControl`]。
pub async fn run(
    spec: &TaskSpec,
    ctrl: &TaskControl,
    tx: &mpsc::UnboundedSender<TaskEvent>,
) -> Result<(), TaskFail> {
    let timeout = Duration::from_secs(spec.timeout_seconds.max(5) as u64);
    let client = build_client(spec.proxy.as_deref(), timeout)
        .map_err(|e| TaskFail::Failed(format!("网络客户端创建失败: {e}")))?;
    // 代理失败自动回退：勾选 Clash 后代理不可达时，切直连继续（对齐 v2 通道回退语义）
    let direct_client = if spec.proxy.is_some() {
        build_client(None, timeout).ok()
    } else {
        None
    };

    // 探测：主源先行（404 → alt 兜底），其余源并发探测（v3 多源聚合）
    let mut primary = spec.url.clone();
    let primary_probe = match probe_resource(&client, &primary, timeout).await {
        Ok(p) => Some(p),
        Err(ProbeFail::NotFound) => {
            if let Some(alt) = spec.alt_url.as_deref() {
                log_evt(tx, LogLevel::Warn, "请求的资源不存在（404），尝试备用地址…".into());
                primary = alt.to_string();
                match probe_resource(&client, &primary, timeout).await {
                    Ok(p) => Some(p),
                    Err(_) => return Err(TaskFail::Failed("下载失败: HTTP 404".into())),
                }
            } else {
                return Err(TaskFail::Failed("下载失败: HTTP 404".into()));
            }
        }
        Err(ProbeFail::Unreachable) => None,
    };

    let mut probes: Vec<(String, Option<Probe>)> = vec![(primary.clone(), primary_probe)];
    if !spec.mirrors.is_empty() {
        let mut js: JoinSet<(usize, Option<Probe>)> = JoinSet::new();
        for (i, m) in spec.mirrors.iter().enumerate() {
            let client = client.clone();
            let m = m.clone();
            js.spawn(async move { (i, probe_resource(&client, &m, timeout).await.ok()) });
        }
        let mut results: Vec<Option<Probe>> = vec![None; spec.mirrors.len()];
        while let Some(res) = js.join_next().await {
            if let Ok((i, p)) = res {
                results[i] = p;
            }
        }
        for (i, m) in spec.mirrors.iter().enumerate() {
            probes.push((m.clone(), results[i].clone()));
        }
    }

    // 裁决：以首个带 total 的可用源为基准，大小不一致的源剔除；主源不可达时升级首个可用镜像
    let accepted_total = probes.iter().find_map(|(_, p)| p.as_ref().and_then(|p| p.total));
    let mut sources: Vec<(String, Option<Probe>)> = Vec::new();
    for (u, p) in probes {
        match p {
            Some(pr) => {
                if let (Some(t), Some(acc)) = (pr.total, accepted_total) {
                    if t != acc {
                        log_evt(tx, LogLevel::Warn, format!("镜像源大小不一致，已剔除: {u}"));
                        continue;
                    }
                }
                sources.push((u, Some(pr)));
            }
            None => {
                // 主源不可达暂留（可能整组都不可达，退回盲下）；镜像不可达直接剔除
                if u == primary {
                    sources.push((u, None));
                } else {
                    log_evt(tx, LogLevel::Warn, format!("镜像源不可用，已剔除: {u}"));
                }
            }
        }
    }
    if sources[0].1.is_none() && sources.len() > 1 {
        let ok_idx = sources[1..].iter().position(|(_, p)| p.is_some()).map(|i| i + 1);
        if let Some(i) = ok_idx {
            let entry = sources.remove(i);
            log_evt(tx, LogLevel::Info, format!("主源不可用，改用镜像源: {}", entry.0));
            sources.insert(0, entry);
        }
    }

    let url = sources[0].0.clone();
    let probe = match sources[0].1.clone() {
        Some(p) => p,
        None => {
            // 探测失败：先试从 sidecar 恢复（保住 part 断点），否则按未知大小盲下
            match restore_probe_from_sidecar(&spec.save_path, &url) {
                Some(p) => {
                    log_evt(tx, LogLevel::Warn, "资源探测失败，按已记录断点信息继续分段续传".into());
                    p
                }
                None => {
                    log_evt(tx, LogLevel::Warn, "资源探测失败，将按未知大小直接下载".into());
                    Probe::default()
                }
            }
        }
    };
    if let Some(name) = &probe.filename {
        log_evt(tx, LogLevel::Info, format!("服务器返回文件名: {name}"));
    }

    // 许可门槛（如 SEGGER）：响应为网页时，解析页面中的许可确认表单并自动提交
    let gate = if probe
        .content_type
        .as_deref()
        .is_some_and(|ct| ct.starts_with("text/html"))
    {
        log_evt(tx, LogLevel::Info, "该站点需要确认使用条款，正在自动处理…".to_string());
        match resolve_license_gate(&client, &url).await {
            Ok(Some(g)) => {
                log_evt(tx, LogLevel::Success, "已自动接受站点使用条款，开始下载".to_string());
                Some(g)
            }
            Ok(None) => {
                log_evt(
                    tx,
                    LogLevel::Error,
                    "链接返回的是网页且未识别到许可确认表单，无法自动下载（请用「直链下载」在浏览器中完成）"
                        .to_string(),
                );
                return Err(TaskFail::Failed("链接返回的是网页而非直接文件".into()));
            }
            Err(e) => {
                log_evt(tx, LogLevel::Warn, format!("许可确认处理失败: {e}"));
                return Err(TaskFail::Failed(format!("链接返回的是网页: {e}")));
            }
        }
    } else {
        None
    };

    let segmented = probe.accept_ranges
        && matches!(probe.total, Some(t) if t >= SEGMENT_MIN_SIZE)
        && spec.segments > 1;

    if segmented {
        let total = probe.total.unwrap_or(0);
        let mut n = (spec.segments as u64).clamp(1, MAX_SEGMENTS as u64);
        if total / n < SEGMENT_MIN_SPAN {
            n = (total / SEGMENT_MIN_SPAN).max(1);
        }
        let source_list: Vec<String> = sources
            .iter()
            .filter_map(|(u, p)| p.as_ref().map(|_| u.clone()))
            .collect();
        let switcher = Arc::new(ProxySwitcher::new(client, direct_client));
        run_segmented(
            &switcher, &url, &spec.save_path, &probe, total, n as u8, ctrl, tx, gate.as_ref(),
            &spec.limits, source_list,
        )
        .await
    } else {
        let switcher = Arc::new(ProxySwitcher::new(client, direct_client));
        run_single(&switcher, &url, &spec.save_path, probe.total, ctrl, tx, gate.as_ref(), &spec.limits)
            .await
    }
}

// ────────────────────────── HTTP 客户端与探测 ──────────────────────────

fn build_client(
    proxy: Option<&str>,
    connect_timeout: Duration,
) -> Result<reqwest::Client, reqwest::Error> {
    let mut builder = reqwest::Client::builder()
        .user_agent(crate::core::USER_AGENT)
        .connect_timeout(connect_timeout);
    if let Some(p) = proxy {
        builder = builder.proxy(reqwest::Proxy::all(p)?);
    }
    builder.build()
}

/// 单次 chunk 读取结果（带 30s 读间隔超时语义）。
enum ReadOutcome {
    Data(Vec<u8>),
    Eof,
    Timeout,
    Err(String),
}

async fn read_chunk(resp: &mut reqwest::Response) -> ReadOutcome {
    // timeout 返回 Result<Result<Option<Bytes>, Error>, Elapsed>
    match tokio::time::timeout(READ_TIMEOUT, resp.chunk()).await {
        Err(_) => ReadOutcome::Timeout,
        Ok(Ok(None)) => ReadOutcome::Eof,
        Ok(Ok(Some(bytes))) => ReadOutcome::Data(bytes.to_vec()),
        Ok(Err(e)) => ReadOutcome::Err(e.to_string()),
    }
}

enum ProbeFail {
    NotFound,
    Unreachable,
}

/// 探测资源大小/Range 支持/ETag：HEAD 优先，被拒或不可达时退化为 `GET Range: bytes=0-0`。
async fn probe_resource(
    client: &reqwest::Client,
    url: &str,
    timeout: Duration,
) -> Result<Probe, ProbeFail> {
    if let Ok(resp) = client.head(url).timeout(timeout).send().await {
        let status = resp.status().as_u16();
        if status == 200 {
            return Ok(probe_from_headers(resp.headers()));
        }
        if status == 404 {
            return Err(ProbeFail::NotFound);
        }
        // 405/501/403 等：HEAD 不被支持，走 GET 探测
    }
    let resp = match client
        .get(url)
        .header("Range", "bytes=0-0")
        .timeout(timeout)
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return Err(ProbeFail::Unreachable),
    };
    match resp.status().as_u16() {
        206 => {
            let mut p = probe_from_headers(resp.headers());
            if let Some(total) = parse_content_range_total(resp.headers()) {
                p.total = Some(total);
            }
            p.accept_ranges = true;
            Ok(p)
        }
        200 => Ok(probe_from_headers(resp.headers())),
        404 => Err(ProbeFail::NotFound),
        _ => Ok(Probe::default()),
    }
}

fn probe_from_headers(headers: &reqwest::header::HeaderMap) -> Probe {
    let total = header_u64(headers, reqwest::header::CONTENT_LENGTH);
    let accept_ranges = headers
        .get(reqwest::header::ACCEPT_RANGES)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("bytes"));
    let etag = headers
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let filename = headers
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_content_disposition);
    let content_type = headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .map(str::to_string);
    Probe {
        total,
        accept_ranges,
        etag,
        filename,
        content_type,
    }
}

fn header_u64(
    headers: &reqwest::header::HeaderMap,
    name: reqwest::header::HeaderName,
) -> Option<u64> {
    headers.get(name)?.to_str().ok()?.parse().ok()
}

/// `Content-Range: bytes 0-0/1234` → 1234。
fn parse_content_range_total(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    let v = headers.get(reqwest::header::CONTENT_RANGE)?.to_str().ok()?;
    v.rsplit('/').next()?.parse::<u64>().ok()
}

/// 从 Content-Disposition 提取文件名；优先 `filename*=UTF-8''…`。
fn parse_content_disposition(v: &str) -> Option<String> {
    for part in v.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("filename*=") {
            if let Some((_charset, value)) = rest.split_once("''") {
                return Some(percent_decode(value.trim_matches('"')));
            }
        }
    }
    for part in v.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("filename=") {
            let name = rest.trim_matches('"');
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// UI 预估大小用的轻量 HEAD；失败返回 None。
pub async fn fetch_size(url: &str, proxy: Option<&str>, timeout_seconds: u32) -> Option<u64> {
    let timeout = Duration::from_secs(timeout_seconds.max(5) as u64);
    let client = build_client(proxy, timeout).ok()?;
    let resp = client.head(url).timeout(timeout).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    header_u64(resp.headers(), reqwest::header::CONTENT_LENGTH)
}

// ────────────────────────── 单流下载（对齐 v1 行为） ──────────────────────────

async fn run_single(
    switcher: &ProxySwitcher,
    url: &str,
    save_path: &Path,
    known_total: Option<u64>,
    ctrl: &TaskControl,
    tx: &mpsc::UnboundedSender<TaskEvent>,
    gate: Option<&GateForm>,
    limits: &SpeedLimits,
) -> Result<(), TaskFail> {
    // 当前使用的客户端；代理不可达时切换到直连（对齐 v2 通道回退语义）
    let mut cur = switcher.current();
    for attempt in 1..=MAX_ATTEMPTS {
        if !ctrl.wait_running().await {
            log_evt(tx, LogLevel::Warn, "下载已取消".into());
            return Err(TaskFail::Cancelled);
        }

        let existing = fs::metadata(save_path).map(|m| m.len()).unwrap_or(0);
        let mut req = build_request(&cur, gate, url, None);
        if existing > 0 {
            req = req.header("Range", format!("bytes={existing}-"));
            log_evt(
                tx,
                LogLevel::Info,
                format!("检测到已有文件，从 {existing} 字节处断点续传"),
            );
        }
        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                log_evt(tx, LogLevel::Error, format!("连接失败: {e}"));
                // 代理通道连接失败：attempt≥2 且存在直连备用时自动降级
                if attempt >= 2 && switcher.has_direct() {
                    switcher.switch_to_direct();
                    cur = switcher.current();
                    log_evt(tx, LogLevel::Proxy, "代理不可达，本任务自动切换直连下载".into());
                }
                match wait_retry(attempt, ctrl, tx).await {
                    Retry::Next => continue,
                    Retry::Cancelled => return Err(TaskFail::Cancelled),
                    Retry::Exhausted => return Err(TaskFail::Failed("自动重试次数用尽".into())),
                }
            }
        };
        let status = resp.status().as_u16();

        // (总大小, 是否追加写入)
        let (total, append) = match status {
            206 => (content_length_of(&resp).map(|cl| existing + cl).or(known_total), true),
            200 => {
                if existing > 0 {
                    log_evt(tx, LogLevel::Warn, "服务器不支持断点续传，重新下载".into());
                }
                (content_length_of(&resp).or(known_total), false)
            }
            416 => {
                let remote = head_size(&cur, url).await;
                if let Some(r) = remote {
                    if existing >= r {
                        log_evt(
                            tx,
                            LogLevel::Success,
                            format!("文件已完整，无需下载 ({existing} 字节)"),
                        );
                        let _ = tx.send(TaskEvent::Progress(Progress {
                            downloaded: existing,
                            total: existing,
                            speed_bps: 0.0,
                            pct: 100.0,
                        }));
                        return Ok(());
                    }
                }
                log_evt(
                    tx,
                    LogLevel::Error,
                    format!("服务器拒绝续传 (416)，本地文件 ({existing} 字节) 与服务器不一致，请删除后重试"),
                );
                return Err(TaskFail::Failed(format!(
                    "HTTP 416（本地 {existing} 字节与服务器不一致）"
                )));
            }
            s if (500..600).contains(&s) => {
                log_evt(tx, LogLevel::Error, format!("连接失败: HTTP {s}"));
                match wait_retry(attempt, ctrl, tx).await {
                    Retry::Next => continue,
                    Retry::Cancelled => return Err(TaskFail::Cancelled),
                    Retry::Exhausted => return Err(TaskFail::Failed("自动重试次数用尽".into())),
                }
            }
            s => {
                log_evt(tx, LogLevel::Error, format!("连接失败: HTTP {s}"));
                return Err(TaskFail::Failed(format!("HTTP {s}")));
            }
        };
        let total = total.unwrap_or(0);

        let mut file = match open_for_write(save_path, append).await {
            Ok(f) => f,
            Err(e) => {
                log_evt(tx, LogLevel::Error, format!("下载出错: 无法打开文件: {e}"));
                return Err(TaskFail::Failed(format!("无法写入文件: {e}")));
            }
        };

        let mut downloaded = if append { existing } else { 0 };
        let mut resp = resp;
        let mut last_tick = Instant::now();
        let mut last_bytes = downloaded;
        let mut interrupted: Option<String> = None;

        loop {
            if !ctrl.wait_running().await {
                log_evt(tx, LogLevel::Warn, "下载已取消".into());
                return Err(TaskFail::Cancelled);
            }
            match read_chunk(&mut resp).await {
                ReadOutcome::Eof => break,
                ReadOutcome::Data(data) => {
                    // 限速在写入侧获取预算；读超时只包裹网络读取，等待不会误触中断判定
                    limits.acquire(data.len()).await;
                    if let Err(e) = file.write_all(&data).await {
                        log_evt(tx, LogLevel::Error, format!("下载出错: 写入失败: {e}"));
                        return Err(TaskFail::Failed(format!("写入文件失败: {e}")));
                    }
                    downloaded += data.len() as u64;
                    if last_tick.elapsed() >= TICK {
                        let dt = last_tick.elapsed().as_secs_f64();
                        let speed = (downloaded - last_bytes) as f64 / dt.max(0.001);
                        let _ = tx.send(TaskEvent::Progress(Progress {
                            downloaded,
                            total,
                            speed_bps: speed,
                            pct: pct_of(downloaded, total),
                        }));
                        last_tick = Instant::now();
                        last_bytes = downloaded;
                    }
                }
                ReadOutcome::Err(e) => {
                    interrupted = Some(format!("下载中断: {e}"));
                    break;
                }
                ReadOutcome::Timeout => {
                    interrupted = Some("下载中断: 连接超时无响应".into());
                    break;
                }
            }
        }
        // 缓冲落盘，保证 part 长度与实际进度一致（续传按文件长度计算）
        let _ = file.flush().await;
        drop(file);

        if interrupted.is_none() && total > 0 && downloaded < total {
            // 传输层提前 EOF：服务器中途断开
            interrupted = Some("下载中断: 连接提前关闭".into());
        }

        match interrupted {
            None => {
                let pct = if total == 0 || downloaded >= total {
                    100.0
                } else {
                    pct_of(downloaded, total)
                };
                let _ = tx.send(TaskEvent::Progress(Progress {
                    downloaded,
                    total: total.max(downloaded),
                    speed_bps: 0.0,
                    pct,
                }));
                log_evt(
                    tx,
                    LogLevel::Success,
                    format!("下载完成: {} ({downloaded} 字节)", save_path.display()),
                );
                return Ok(());
            }
            Some(msg) => {
                // 已写入的部分保留，交给下一轮续传
                log_evt(tx, LogLevel::Warn, msg);
                match wait_retry(attempt, ctrl, tx).await {
                    Retry::Next => {}
                    Retry::Cancelled => return Err(TaskFail::Cancelled),
                    Retry::Exhausted => return Err(TaskFail::Failed("自动重试次数用尽".into())),
                }
            }
        }
    }
    Err(TaskFail::Failed("自动重试次数用尽".into()))
}

/// 追加模式续传，或截断重下。append 与 truncate 不能同时设置，须分开构造。
async fn open_for_write(path: &Path, append: bool) -> std::io::Result<tokio::fs::File> {
    if append {
        tokio::fs::OpenOptions::new().create(true).append(true).open(path).await
    } else {
        tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .await
    }
}

// ────────────────────────── 分段下载 ──────────────────────────

#[derive(Debug, Clone, Copy)]
struct Seg {
    start: u64,
    end: u64, // 含端点
}

impl Seg {
    fn len(&self) -> u64 {
        self.end - self.start + 1
    }
}

fn plan_segments(total: u64, n: u8) -> Vec<Seg> {
    let n = n as u64;
    let base = total / n;
    let rem = total % n;
    let mut segs = Vec::with_capacity(n as usize);
    let mut start = 0;
    for i in 0..n {
        let len = base + u64::from(i < rem);
        segs.push(Seg {
            start,
            end: start + len - 1,
        });
        start += len;
    }
    segs
}

#[derive(Serialize, Deserialize, Clone)]
struct Sidecar {
    url: String,
    etag: Option<String>,
    total: u64,
    segments: u8,
    /// v3 多源：全部源 URL（单源任务仅含主源；缺省兼容 v2 旧文件）
    #[serde(default)]
    urls: Vec<String>,
}

/// 读取 sidecar（v3 `.vdl.json` 优先，兼容 v2 `.ghdl.json`）。
fn read_sidecar_any(save_path: &Path) -> Option<Sidecar> {
    fs::read(sidecar_path(save_path))
        .ok()
        .and_then(|d| serde_json::from_slice(&d).ok())
        .or_else(|| {
            fs::read(v2_sidecar_path(save_path))
                .ok()
                .and_then(|d| serde_json::from_slice(&d).ok())
        })
}

/// 探测失败时从已记录的 sidecar 恢复资源信息：url 一致且大小已知，
/// 则按已知大小继续分段续传——避免临时网络/代理故障导致 part 断点作废从头重下。
fn restore_probe_from_sidecar(save_path: &Path, url: &str) -> Option<Probe> {
    let sidecar = read_sidecar_any(save_path)?;
    if sidecar.url != url || sidecar.total == 0 {
        return None;
    }
    Some(Probe {
        total: Some(sidecar.total),
        accept_ranges: true,
        etag: sidecar.etag,
        filename: None,
        content_type: None,
    })
}

fn sibling_path(save_path: &Path, suffix: &str) -> PathBuf {
    let name = save_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    save_path.with_file_name(format!("{name}{suffix}"))
}

fn part_path(save_path: &Path, i: usize) -> PathBuf {
    sibling_path(save_path, &format!(".part{i}"))
}

fn sidecar_path(save_path: &Path) -> PathBuf {
    sibling_path(save_path, ".vdl.json")
}

/// v2 遗留 sidecar（同结构）；命中即可续传 v2 未完成的下载。
fn v2_sidecar_path(save_path: &Path) -> PathBuf {
    sibling_path(save_path, ".ghdl.json")
}

/// 清理分段残留（part 文件与 sidecar；含 v2 遗留 sidecar）。
fn cleanup_parts(save_path: &Path, segments: u8) {
    for i in 0..segments as usize {
        let _ = fs::remove_file(part_path(save_path, i));
    }
    let _ = fs::remove_file(sidecar_path(save_path));
    let _ = fs::remove_file(v2_sidecar_path(save_path));
}

enum SegFail {
    /// 暂停或停止导致的安全退出
    Stopped,
    /// 服务器忽略 Range 返回了整文件内容，分段不可行
    NoRange,
    /// 源侧失败（连接错/5xx/4xx），多源模式下交调度器改派他源
    SourceDown,
    Fatal(String),
}

/// 未采样源的虚拟速率（中性偏低，采样过的快源自然排前）。
const SOURCE_DEFAULT_SCORE: f64 = 500_000.0;
/// 连续失败达到该次数后将源标记为死亡（多源模式下改派其余源）。
const SOURCE_DEAD_AFTER_FAILS: u32 = 2;

#[derive(Clone)]
struct SourceSlot {
    url: String,
    /// EMA 速率 bytes/s
    ema: f64,
    sampled: bool,
    consecutive_fails: u32,
    alive: bool,
}

/// 分段与源的全局调度表。锁内无 await；状态变化以 Notify 广播。
struct MultiState {
    segs: Vec<Seg>,
    done: Vec<bool>,
    claimed: Vec<bool>,
    next_claim: usize,
    /// 平局轮换游标（初轮各源均摊，之后 EMA 决定）
    rr: usize,
    attempts: Vec<u32>,
    sources: Vec<SourceSlot>,
    no_range: bool,
    fatal: Option<String>,
    notify: std::sync::Arc<tokio::sync::Notify>,
}

enum Claim {
    /// 领到 (分段下标, 源下标)
    Work(usize, usize),
    /// 目标分段都在途，等通知后重查
    Wait,
    /// 无存活源且无在途分段
    NoSource,
    /// 全部完成或已触发终止标志
    Finished,
}

impl MultiState {
    fn new(segs: Vec<Seg>, sources: Vec<String>) -> Self {
        Self {
            done: vec![false; segs.len()],
            claimed: vec![false; segs.len()],
            next_claim: 0,
            rr: 0,
            attempts: vec![0; segs.len()],
            sources: sources
                .into_iter()
                .map(|url| SourceSlot {
                    url,
                    ema: 0.0,
                    sampled: false,
                    consecutive_fails: 0,
                    alive: true,
                })
                .collect(),
            segs,
            no_range: false,
            fatal: None,
            notify: std::sync::Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// 选最快存活源；同分平局按轮换游标均摊（首轮无采样时不偏袒任何源）。
    fn best_alive(&mut self) -> Option<usize> {
        let mut best_score = f64::NEG_INFINITY;
        let mut tied: Vec<usize> = Vec::new();
        for (i, s) in self.sources.iter().enumerate() {
            if !s.alive {
                continue;
            }
            let score = if s.sampled { s.ema } else { SOURCE_DEFAULT_SCORE };
            if (score - best_score).abs() < f64::EPSILON || score > best_score {
                if score > best_score {
                    best_score = score;
                    tied.clear();
                }
                tied.push(i);
            }
        }
        if tied.is_empty() {
            return None;
        }
        let pick = tied[self.rr % tied.len()];
        self.rr += 1;
        Some(pick)
    }

    fn claim(&mut self) -> Claim {
        if self.no_range || self.fatal.is_some() {
            return Claim::Finished;
        }
        if self.done.iter().all(|&d| d) {
            return Claim::Finished;
        }
        for _ in 0..self.segs.len() {
            let i = self.next_claim % self.segs.len();
            self.next_claim += 1;
            if !self.done[i] && !self.claimed[i] {
                if let Some(s) = self.best_alive() {
                    self.claimed[i] = true;
                    self.attempts[i] += 1;
                    return Claim::Work(i, s);
                }
                // 有分段待领但没有存活源：在途段可能还会成功，先等再判死刑
                let in_flight = self.claimed.iter().filter(|&&c| c).count();
                return if in_flight == 0 {
                    Claim::NoSource
                } else {
                    Claim::Wait
                };
            }
        }
        Claim::Wait
    }

    fn release(&mut self, i: usize) {
        self.claimed[i] = false;
        self.notify.notify_waiters();
    }

    fn complete(&mut self, i: usize, src: usize, bytes: u64, secs: f64) {
        self.done[i] = true;
        let s = &mut self.sources[src];
        let inst = bytes as f64 / secs.max(0.001);
        s.ema = if s.sampled { s.ema * 0.6 + inst * 0.4 } else { inst };
        s.sampled = true;
        s.consecutive_fails = 0;
        self.notify.notify_waiters();
    }

    fn punish(&mut self, src: usize) {
        let s = &mut self.sources[src];
        s.consecutive_fails += 1;
        s.ema *= 0.5;
        if s.consecutive_fails >= SOURCE_DEAD_AFTER_FAILS {
            s.alive = false;
        }
        self.notify.notify_waiters();
    }

    fn mark_no_range(&mut self) {
        self.no_range = true;
        self.notify.notify_waiters();
    }

    fn mark_fatal(&mut self, e: String) {
        if self.fatal.is_none() {
            self.fatal = Some(e);
        }
        self.notify.notify_waiters();
    }
}

/// Clash 代理切换器：默认走代理客户端；代理失败可一键切直连（全 worker 生效）。
struct ProxySwitcher {
    with_proxy: reqwest::Client,
    direct: Option<reqwest::Client>,
    use_direct: std::sync::atomic::AtomicBool,
    logged: std::sync::atomic::AtomicBool,
}

impl ProxySwitcher {
    fn new(with_proxy: reqwest::Client, direct: Option<reqwest::Client>) -> Self {
        Self {
            with_proxy,
            direct,
            use_direct: std::sync::atomic::AtomicBool::new(false),
            logged: std::sync::atomic::AtomicBool::new(false),
        }
    }

    fn has_direct(&self) -> bool {
        self.direct.is_some()
    }

    fn current(&self) -> reqwest::Client {
        if self.use_direct.load(std::sync::atomic::Ordering::Relaxed) {
            self.direct.clone().unwrap_or_else(|| self.with_proxy.clone())
        } else {
            self.with_proxy.clone()
        }
    }

    /// 标记切换直连；返回 true 表示本次调用完成了切换（用于只记一次日志）。
    fn switch_to_direct(&self) -> bool {
        if !self.has_direct() {
            return false;
        }
        let first = !self.use_direct.swap(true, std::sync::atomic::Ordering::Relaxed);
        if first {
            self.logged.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        first
    }
}

#[derive(Debug)]
enum WorkerFail {
    Stopped,
    NoRange,
    Fatal(String),
}

/// 工人循环：领 (段, 源) → 下载 → 完成回写/失败惩罚改派，直至全部完成或终止。
#[allow(clippy::too_many_arguments)]
async fn multi_worker(
    switcher: Arc<ProxySwitcher>,
    save_path: PathBuf,
    ctrl: TaskControl,
    tx: mpsc::UnboundedSender<TaskEvent>,
    gate: Option<GateForm>,
    limits: SpeedLimits,
    written: Arc<AtomicU64>,
    state: Arc<Mutex<MultiState>>,
) -> Result<(), WorkerFail> {
    loop {
        if !ctrl.wait_running().await {
            return Err(WorkerFail::Stopped);
        }
        {
            let st = state.lock().unwrap();
            if st.no_range {
                return Err(WorkerFail::NoRange);
            }
            if let Some(f) = &st.fatal {
                return Err(WorkerFail::Fatal(f.clone()));
            }
        }
        let claim = state.lock().unwrap().claim();
        match claim {
            Claim::Finished => return Ok(()),
            Claim::NoSource => {
                return Err(WorkerFail::Fatal("所有下载源均不可用".into()));
            }
            Claim::Wait => {
                // 等在途段的通知；500ms 超时兜底重查（防通知丢失）
                let notify = state.lock().unwrap().notify.clone();
                let _ = tokio::time::timeout(Duration::from_millis(500), notify.notified()).await;
                continue;
            }
            Claim::Work(i, s) => {
                let seg = state.lock().unwrap().segs[i];
                let url = state.lock().unwrap().sources[s].url.clone();
                {
                    let mut st = state.lock().unwrap();
                    st.attempts[i] += 1;
                    if st.attempts[i] > MAX_ATTEMPTS {
                        let msg = format!("分段 {i} 自动重试次数用尽");
                        st.mark_fatal(msg.clone());
                        return Err(WorkerFail::Fatal(msg));
                    }
                }
                let start = Instant::now();
                // 每次尝试前取当前客户端（代理断开被切换后自动走直连）
                let cur = switcher.current();
                match run_segment_from(
                    cur,
                    url,
                    part_path(&save_path, i),
                    seg,
                    ctrl.clone(),
                    written.clone(),
                    gate.clone(),
                    tx.clone(),
                    limits.clone(),
                    false,
                )
                .await
                {
                    Ok(()) => {
                        state.lock().unwrap().complete(
                            i,
                            s,
                            seg.len(),
                            start.elapsed().as_secs_f64(),
                        );
                    }
                    Err(SegFail::Stopped) => {
                        state.lock().unwrap().release(i);
                        return Err(WorkerFail::Stopped);
                    }
                    Err(SegFail::NoRange) => {
                        {
                            let mut st = state.lock().unwrap();
                            st.mark_no_range();
                            st.release(i);
                        }
                        return Err(WorkerFail::NoRange);
                    }
                    Err(SegFail::SourceDown) => {
                        state.lock().unwrap().punish(s);
                        state.lock().unwrap().release(i);
                    }
                    Err(SegFail::Fatal(e)) => {
                        state.lock().unwrap().release(i);
                        state.lock().unwrap().mark_fatal(e.clone());
                        return Err(WorkerFail::Fatal(e));
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_segmented(
    switcher: &std::sync::Arc<ProxySwitcher>,
    url: &str,
    save_path: &Path,
    probe: &Probe,
    total: u64,
    segments: u8,
    ctrl: &TaskControl,
    tx: &mpsc::UnboundedSender<TaskEvent>,
    gate: Option<&GateForm>,
    limits: &SpeedLimits,
    sources: Vec<String>,
) -> Result<(), TaskFail> {
    let multi = sources.len() > 1;
    // 多源模式段数×2：留出调度余地，快源能认领更多分段
    let seg_count = if multi { segments * 2 } else { segments };
    let segs = plan_segments(total, seg_count);

    // 复用未完成的分段：sidecar 与本次任务一致才可信（兼容 v2 的 .ghdl.json）
    let sidecar: Option<Sidecar> = read_sidecar_any(save_path);
    let reusable = sidecar.as_ref().is_some_and(|s| {
        s.url == url && s.total == total && s.segments == seg_count && s.etag == probe.etag
    });
    if !reusable {
        if (0..seg_count as usize).any(|i| part_len(save_path, i) > 0) {
            log_evt(tx, LogLevel::Warn, "已有分段文件与服务器不一致，重新下载".into());
        }
        cleanup_parts(save_path, seg_count);
        let sidecar = Sidecar {
            url: url.to_string(),
            etag: probe.etag.clone(),
            total,
            segments: seg_count,
            urls: sources.clone(),
        };
        if let Ok(json) = serde_json::to_vec(&sidecar) {
            let _ = fs::write(sidecar_path(save_path), json);
        }
    } else {
        let done = (0..seg_count as usize)
            .filter(|&i| part_len(save_path, i) >= segs[i].len())
            .count();
        if done > 0 {
            log_evt(
                tx,
                LogLevel::Info,
                format!("检测到未完成的分段下载，其中 {done}/{seg_count} 段已完成"),
            );
        }
    }

    let mut ema_speed = 0.0f64;
    loop {
        if !ctrl.wait_running().await {
            return Err(TaskFail::Cancelled);
        }
        let base: Vec<u64> = (0..seg_count as usize)
            .map(|i| part_len(save_path, i).min(segs[i].len()))
            .collect();
        let base_sum: u64 = base.iter().sum();
        let written = Arc::new(AtomicU64::new(0));

        let state = Arc::new(Mutex::new(MultiState::new(segs.clone(), sources.clone())));
        // part 已完整的段直接标 done，调度器不会再派发
        for i in 0..seg_count as usize {
            if base[i] >= segs[i].len() {
                state.lock().unwrap().done[i] = true;
            }
        }

        let worker_count = segments as usize;
        let mut join_set: JoinSet<Result<(), WorkerFail>> = JoinSet::new();
        for _ in 0..worker_count {
            join_set.spawn(multi_worker(
                std::sync::Arc::clone(&switcher),
                save_path.to_path_buf(),
                ctrl.clone(),
                tx.clone(),
                gate.cloned(),
                limits.clone(),
                written.clone(),
                state.clone(),
            ));
        }

        // 进度 tick + 收割工人结果
        let mut fatal: Option<String> = None;
        let mut no_range = false;
        let mut stopped = false;
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut last_tick = Instant::now();
        let mut last_bytes = base_sum;
        while !join_set.is_empty() {
            tokio::select! {
                res = join_set.join_next(), if !join_set.is_empty() => {
                    match res {
                        Some(Ok(Ok(()))) => {}
                        Some(Ok(Err(WorkerFail::Stopped))) => stopped = true,
                        Some(Ok(Err(WorkerFail::NoRange))) => no_range = true,
                        Some(Ok(Err(WorkerFail::Fatal(e)))) => fatal = Some(e),
                        Some(Err(_)) => fatal = Some("工人任务异常退出".into()),
                        None => {}
                    }
                }
                _ = interval.tick() => {
                    if last_tick.elapsed() >= TICK {
                        let downloaded = base_sum + written.load(Ordering::Relaxed);
                        if downloaded == last_bytes {
                            // 无新字节：速度归零（避免 EMA 无限衰减出浮点下溢垃圾值）
                            ema_speed = 0.0;
                        } else {
                            let inst = (downloaded - last_bytes) as f64
                                / last_tick.elapsed().as_secs_f64().max(0.001);
                            ema_speed = if ema_speed == 0.0 {
                                inst
                            } else {
                                ema_speed * 0.6 + inst * 0.4
                            };
                        }
                        let _ = tx.send(TaskEvent::Progress(Progress {
                            downloaded,
                            total,
                            speed_bps: ema_speed,
                            pct: pct_of(downloaded, total),
                        }));
                        last_tick = Instant::now();
                        last_bytes = downloaded;
                    }
                }
            }
        }

        if let Some(e) = fatal {
            return Err(TaskFail::Failed(e));
        }
        if no_range {
            log_evt(tx, LogLevel::Warn, "服务器不支持分段下载，退回单线程模式".into());
            cleanup_parts(save_path, seg_count);
            return run_single(switcher, url, save_path, Some(total), ctrl, tx, gate, limits).await;
        }
        if stopped {
            if ctrl.state() == Ctrl::Stopping {
                log_evt(tx, LogLevel::Warn, "下载已取消".into());
                return Err(TaskFail::Cancelled);
            }
            // Paused：等待恢复后重新派工（part 进度保留）
            if !ctrl.wait_running().await {
                log_evt(tx, LogLevel::Warn, "下载已取消".into());
                return Err(TaskFail::Cancelled);
            }
            log_evt(tx, LogLevel::Info, "继续下载".into());
            continue;
        }
        // 全部段完成
        break;
    }

    // 合并
    let mut final_file = match tokio::fs::File::create(save_path).await {
        Ok(f) => f,
        Err(e) => return Err(TaskFail::Failed(format!("无法创建文件: {e}"))),
    };
    for (i, seg) in segs.iter().enumerate() {
        if part_len(save_path, i) != seg.len() {
            return Err(TaskFail::Failed("分段文件不完整，请重试".into()));
        }
        let mut part = match tokio::fs::File::open(part_path(save_path, i)).await {
            Ok(f) => f,
            Err(e) => return Err(TaskFail::Failed(format!("读取分段失败: {e}"))),
        };
        if let Err(e) = tokio::io::copy(&mut part, &mut final_file).await {
            return Err(TaskFail::Failed(format!("合并分段失败: {e}")));
        }
        let _ = fs::remove_file(part_path(save_path, i));
    }
    let _ = final_file.flush().await;
    drop(final_file);
    let _ = fs::remove_file(sidecar_path(save_path));
    let _ = tx.send(TaskEvent::Progress(Progress {
        downloaded: total,
        total,
        speed_bps: 0.0,
        pct: 100.0,
    }));
    log_evt(
        tx,
        LogLevel::Success,
        format!("下载完成: {} ({total} 字节)", save_path.display()),
    );
    Ok(())
}

/// 从单一源下载一个分段。
///
/// `retry_same = true`（单源任务）保持 v2 语义：网络错/5xx 睡 5s 同源重试至多 8 次；
/// `false`（多源任务）：任何源侧失败立即返回 [`SegFail::SourceDown`]，由调度器改派他源
/// （已写入部分保留，改派按 part 现有长度续传）。
#[allow(clippy::too_many_arguments)]
async fn run_segment_from(
    client: reqwest::Client,
    url: String,
    part: PathBuf,
    seg: Seg,
    ctrl: TaskControl,
    written: Arc<AtomicU64>,
    gate: Option<GateForm>,
    tx: mpsc::UnboundedSender<TaskEvent>,
    limits: SpeedLimits,
    retry_same: bool,
) -> Result<(), SegFail> {
    let seg_len = seg.len();
    let mut attempt = 0;
    loop {
        attempt += 1;
        if !ctrl.wait_running().await {
            return Err(SegFail::Stopped);
        }
        let existing = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        if existing > seg_len {
            return Err(SegFail::Fatal(format!(
                "分段文件超过预期大小（{} > {seg_len}）",
                part.display()
            )));
        }
        if existing == seg_len {
            return Ok(());
        }

        let req = build_request(&client, gate.as_ref(), &url, Some(format!(
            "bytes={}-{}",
            seg.start + existing,
            seg.end
        )));
        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                log_evt(&tx, LogLevel::Warn, format!("分段连接失败: {e}"));
                if !retry_same {
                    return Err(SegFail::SourceDown);
                }
                match wait_retry(attempt, &ctrl, &tx).await {
                    Retry::Next => continue,
                    Retry::Cancelled => return Err(SegFail::Stopped),
                    Retry::Exhausted => {
                        return Err(SegFail::Fatal("分段自动重试次数用尽".into()))
                    }
                }
            }
        };
        match resp.status().as_u16() {
            206 => {}
            200 => return Err(SegFail::NoRange),
            416 => {
                if existing == seg_len {
                    return Ok(());
                }
                return Err(SegFail::Fatal(format!(
                    "服务器拒绝分段续传 (416)，分段 {part:?}"
                )));
            }
            s if (500..600).contains(&s) => {
                log_evt(&tx, LogLevel::Warn, format!("分段下载遇到 HTTP {s}"));
                if !retry_same {
                    return Err(SegFail::SourceDown);
                }
                match wait_retry(attempt, &ctrl, &tx).await {
                    Retry::Next => continue,
                    Retry::Cancelled => return Err(SegFail::Stopped),
                    Retry::Exhausted => {
                        return Err(SegFail::Fatal("分段自动重试次数用尽".into()))
                    }
                }
            }
            s => {
                if !retry_same {
                    log_evt(&tx, LogLevel::Warn, format!("分段下载遇到 HTTP {s}，换源重试"));
                    return Err(SegFail::SourceDown);
                }
                return Err(SegFail::Fatal(format!("分段下载失败: HTTP {s}")));
            }
        }

        let mut resp = resp;
        let mut file = match tokio::fs::OpenOptions::new().create(true).append(true).open(&part).await
        {
            Ok(f) => f,
            Err(e) => return Err(SegFail::Fatal(format!("无法写入分段文件: {e}"))),
        };
        let mut done = existing;
        let mut interrupted = false;
        loop {
            if !ctrl.wait_running().await {
                return Err(SegFail::Stopped);
            }
            match read_chunk(&mut resp).await {
                ReadOutcome::Eof => break,
                ReadOutcome::Data(data) => {
                    // 限速在写入侧获取预算（与单流路径同语义）
                    limits.acquire(data.len()).await;
                    if let Err(e) = file.write_all(&data).await {
                        return Err(SegFail::Fatal(format!("写入分段失败: {e}")));
                    }
                    done += data.len() as u64;
                    written.fetch_add(data.len() as u64, Ordering::Relaxed);
                    if done > seg_len {
                        return Err(SegFail::Fatal("服务器返回数据超出分段范围".into()));
                    }
                }
                ReadOutcome::Err(e) => {
                    log_evt(&tx, LogLevel::Warn, format!("分段下载中断: {e}"));
                    interrupted = true;
                    break;
                }
                ReadOutcome::Timeout => {
                    log_evt(&tx, LogLevel::Warn, "分段下载中断: 连接超时无响应".into());
                    interrupted = true;
                    break;
                }
            }
        }
        let _ = file.flush().await;
        drop(file);
        if !interrupted && done == seg_len {
            return Ok(());
        }
        if !interrupted {
            log_evt(&tx, LogLevel::Warn, "分段连接提前关闭".into());
        }
        if !retry_same {
            // 多源：传输中断同样改派他源（已写部分保留）
            return Err(SegFail::SourceDown);
        }
        match wait_retry(attempt, &ctrl, &tx).await {
            Retry::Next => {}
            Retry::Cancelled => return Err(SegFail::Stopped),
            Retry::Exhausted => return Err(SegFail::Fatal("分段自动重试次数用尽".into())),
        }
    }
}

// ────────────────────────── 许可门槛（SEGGER 类站点） ──────────────────────────

/// 按许可表单/断点需求构造下载请求。
fn build_request<'a>(
    client: &'a reqwest::Client,
    gate: Option<&GateForm>,
    url: &str,
    range: Option<String>,
) -> reqwest::RequestBuilder {
    let rb = match gate {
        Some(g) => client
            .post(&g.action)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(g.body.clone()),
        None => client.get(url),
    };
    match range {
        Some(r) => rb.header("Range", r),
        None => rb,
    }
}

/// 解析许可门槛页面：找到「method=post 且含 accept 类字段」的表单，
/// 返回可直接提交的 GateForm；页面无此表单返回 None。
async fn resolve_license_gate(
    client: &reqwest::Client,
    page_url: &str,
) -> Result<Option<GateForm>, String> {
    let resp = client
        .get(page_url)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("获取页面失败: {e}"))?;
    if resp.status().is_server_error() {
        return Err("站点返回 5xx".into());
    }
    let html = resp.text().await.map_err(|e| format!("读取页面失败: {e}"))?;
    Ok(extract_post_accept_form(&html, page_url))
}

/// 从 HTML 中提取「method=post 且含 accept 类字段」的表单。
fn extract_post_accept_form(html: &str, page_url: &str) -> Option<GateForm> {
    let lower = html.to_lowercase();
    let mut search = 0usize;
    while let Some(rel) = lower[search..].find("<form") {
        let tag_start = search + rel;
        let Some(tag_end_rel) = lower[tag_start..].find('>') else {
            break;
        };
        let tag_end = tag_start + tag_end_rel;
        let tag = &lower[tag_start..=tag_end];
        let tag_orig = &html[tag_start..=tag_end];
        let is_post = tag.contains("method=\"post\"") || tag.contains("method='post'");
        // action 从原始大小写提取：URL 路径区分大小写
        let action = extract_attr(&tag_orig, "action").unwrap_or_default();
        let Some(body_end_rel) = lower[tag_end..].find("</form>") else {
            break;
        };
        let body_end = tag_end + body_end_rel;
        let _body = &html[tag_start..body_end];

        let mut fields: Vec<(String, String)> = Vec::new();
        let mut has_accept = false;
        let mut pos = tag_end;
        while let Some(rel) = lower[pos..body_end].find("<input") {
            let inp_start = pos + rel;
            let Some(inp_end_rel) = lower[inp_start..body_end].find('>') else {
                break;
            };
            let inp_end = inp_start + inp_end_rel;
            let inp = &html[inp_start..=inp_end];
            if let Some(name) = extract_attr(inp, "name") {
                let value = extract_attr(inp, "value").unwrap_or_else(|| "on".into());
                if name.to_lowercase().contains("accept") {
                    has_accept = true;
                }
                fields.push((name, value));
            }
            pos = inp_end;
        }

        if is_post && has_accept {
            return Some(GateForm {
                action: resolve_action_url(page_url, &action),
                body: urlencode(&fields),
            });
        }
        search = body_end;
    }
    None
}

/// 从标签文本提取属性值（容忍单双引号）。
fn extract_attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    let pat = format!("{name}=");
    let i = lower.find(&pat)? + pat.len();
    let rest = &tag[i..];
    let quote = rest.chars().next()?;
    let value = if quote == '"' || quote == '\'' {
        let end = rest[1..].find(quote)? + 1;
        &rest[1..end]
    } else {
        let end = rest.find([' ', '>']).unwrap_or(rest.len());
        &rest[..end]
    };
    Some(value.trim().to_string())
}

/// 相对 action 解析为绝对 URL。
fn resolve_action_url(page_url: &str, action: &str) -> String {
    if action.starts_with("http://") || action.starts_with("https://") {
        return action.to_string();
    }
    // scheme 后（含 "://" 共 3 个字符）再找下一个 '/'，即为源点结束
    let origin = page_url
        .find("://")
        .map(|i| {
            let after = i + 3;
            page_url[after..]
                .find('/')
                .map(|s| after + s)
                .unwrap_or(page_url.len())
        })
        .map(|e| page_url[..e].to_string())
        .unwrap_or_default();
    if let Some(rest) = action.strip_prefix('/') {
        format!("{origin}/{rest}")
    } else {
        let dir_end = page_url.rfind('/').unwrap_or(0);
        format!("{}/{}", &page_url[..dir_end], action)
    }
}

/// 极简表单编码（空格 → %20，其余非安全字符百分号编码）。
fn urlencode(fields: &[(String, String)]) -> String {
    let mut out = String::new();
    for (n, v) in fields {
        if !out.is_empty() {
            out.push('&');
        }
        out.push_str(&percent_encode(n));
        out.push('=');
        out.push_str(&percent_encode(v));
    }
    out
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ────────────────────────── 小工具 ──────────────────────────

enum Retry {
    Next,
    Cancelled,
    Exhausted,
}

/// 重试等待：间隔内每 100ms 检查一次停止请求。
async fn wait_retry(attempt: u32, ctrl: &TaskControl, tx: &mpsc::UnboundedSender<TaskEvent>) -> Retry {
    if attempt >= MAX_ATTEMPTS {
        return Retry::Exhausted;
    }
    log_evt(
        tx,
        LogLevel::Warn,
        format!(
            "{} 秒后自动重试（第 {attempt}/{} 次）...",
            RETRY_DELAY.as_secs(),
            MAX_ATTEMPTS - 1
        ),
    );
    let slices = (RETRY_DELAY.as_millis() / 100) as u32;
    for _ in 0..slices {
        if ctrl.state() == Ctrl::Stopping {
            return Retry::Cancelled;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Retry::Next
}

fn content_length_of(resp: &reqwest::Response) -> Option<u64> {
    header_u64(resp.headers(), reqwest::header::CONTENT_LENGTH)
}

async fn head_size(client: &reqwest::Client, url: &str) -> Option<u64> {
    let resp = client.head(url).timeout(Duration::from_secs(15)).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    content_length_of(&resp)
}

fn part_len(save_path: &Path, i: usize) -> u64 {
    fs::metadata(part_path(save_path, i))
        .map(|m| m.len())
        .unwrap_or(0)
}

fn pct_of(downloaded: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        downloaded as f64 / total as f64 * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_plan_covers_exactly() {
        for &(total, n) in &[(100u64, 3u8), (1024, 7), (1, 1), (9, 4), (8 * 1024 * 1024, 8)] {
            let segs = plan_segments(total, n);
            assert_eq!(segs.len(), n as usize);
            let mut expect = 0;
            for s in &segs {
                assert_eq!(s.start, expect);
                assert!(s.end >= s.start);
                expect = s.end + 1;
            }
            assert_eq!(expect, total);
        }
    }

    #[test]
    fn content_disposition_variants() {
        assert_eq!(
            parse_content_disposition(r#"attachment; filename="setup.exe""#).as_deref(),
            Some("setup.exe")
        );
        assert_eq!(
            parse_content_disposition("attachment; filename*=UTF-8''%E4%B8%AD%E6%96%87.zip").as_deref(),
            Some("中文.zip")
        );
        assert_eq!(parse_content_disposition("attachment"), None);
    }

    #[tokio::test]
    async fn control_pause_resume_stop() {
        let ctrl = TaskControl::new();
        assert_eq!(ctrl.state(), Ctrl::Running);
        ctrl.pause();
        assert_eq!(ctrl.state(), Ctrl::Paused);
        ctrl.resume();
        assert_eq!(ctrl.state(), Ctrl::Running);
        assert!(ctrl.wait_running().await);
        ctrl.stop();
        assert!(!ctrl.wait_running().await);
    }

    #[test]
    fn sibling_paths() {
        let p = Path::new("D:/dl/app.zip");
        assert_eq!(part_path(p, 2), Path::new("D:/dl/app.zip.part2"));
        assert_eq!(sidecar_path(p), Path::new("D:/dl/app.zip.vdl.json"));
        assert_eq!(v2_sidecar_path(p), Path::new("D:/dl/app.zip.ghdl.json"));
    }

    #[test]
    fn restore_probe_from_sidecar_requires_url_match() {
        let p = tmp_file("restore-sidecar");
        // 无 sidecar → 不可恢复
        assert!(restore_probe_from_sidecar(&p, "https://x/a").is_none());
        let sc = Sidecar {
            url: "https://x/a".into(),
            etag: Some("\"e\"".into()),
            total: 9_000_000,
            segments: 8,
            urls: vec![],
        };
        fs::write(sidecar_path(&p), serde_json::to_vec(&sc).unwrap()).unwrap();
        let probe = restore_probe_from_sidecar(&p, "https://x/a").unwrap();
        assert_eq!(probe.total, Some(9_000_000));
        assert!(probe.accept_ranges);
        assert_eq!(probe.etag.as_deref(), Some("\"e\""));
        // 不同 url（资源已变更）→ 不可信，盲下从头
        assert!(restore_probe_from_sidecar(&p, "https://x/b").is_none());
        let _ = fs::remove_file(sidecar_path(&p));
        let _ = fs::remove_file(&p);
    }

    // ────────────── 集成测试：本地假 HTTP 服务器（async） ──────────────

    use std::io::Error as IoError;
    use std::sync::atomic::AtomicUsize;
    use tokio::io::AsyncBufReadExt;

    use crate::engine::limiter::Limiter;

    struct FakeReq {
        method: String,
        range: Option<String>,
    }

    struct FakeResp {
        status: u16,
        reason: &'static str,
        headers: Vec<(&'static str, String)>,
        body: Vec<u8>,
        /// 只发送前 N 字节后断开，模拟连接中断
        truncate_at: Option<u64>,
    }

    impl FakeResp {
        fn ok(headers: Vec<(&'static str, String)>, body: Vec<u8>) -> Self {
            Self {
                status: 200,
                reason: "OK",
                headers,
                body,
                truncate_at: None,
            }
        }

        fn partial(total: u64, full: &[u8], start: u64, end: u64) -> Self {
            Self {
                status: 206,
                reason: "Partial Content",
                headers: vec![("Content-Range", format!("bytes {start}-{end}/{total}"))],
                body: full[start as usize..=(end as usize).min(full.len() - 1)].to_vec(),
                truncate_at: None,
            }
        }
    }

    fn test_body(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 7 % 251) as u8).collect()
    }

    /// 启动一个处理 n_requests 个请求的假服务器，返回基础 URL。
    async fn serve<F>(n_requests: usize, handler: F) -> String
    where
        F: Fn(usize, &FakeReq) -> FakeResp + Send + Sync + 'static,
    {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("绑定测试端口");
        let addr = listener.local_addr().unwrap();
        let handler = Arc::new(handler);
        tokio::spawn(async move {
            for i in 0..n_requests {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                if let Err(e) = handle_fake_conn(stream, i, handler.as_ref()).await {
                    eprintln!("[fake-server] 连接处理失败: {e}");
                }
            }
        });
        format!("http://{addr}/file.bin")
    }

    async fn handle_fake_conn<F>(
        stream: tokio::net::TcpStream,
        idx: usize,
        handler: &F,
    ) -> Result<(), IoError>
    where
        F: Fn(usize, &FakeReq) -> FakeResp,
    {
        let (reader, mut writer) = stream.into_split();
        let mut reader = tokio::io::BufReader::new(reader);
        let mut first = String::new();
        reader.read_line(&mut first).await?;
        let method = first
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string();
        let mut range = None;
        loop {
            let mut h = String::new();
            if reader.read_line(&mut h).await? == 0 {
                break;
            }
            let t = h.trim_end();
            if t.is_empty() {
                break;
            }
            if let Some(v) = t.to_ascii_lowercase().strip_prefix("range:") {
                range = Some(v.trim().to_string());
            }
        }
        let resp = handler(idx, &FakeReq { method, range });
        let mut out = format!(
            "HTTP/1.1 {} {}\r\nConnection: close\r\n",
            resp.status, resp.reason
        );
        for (k, v) in &resp.headers {
            out.push_str(&format!("{k}: {v}\r\n"));
        }
        if !resp
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        {
            out.push_str(&format!("Content-Length: {}\r\n", resp.body.len()));
        }
        out.push_str("\r\n");
        writer.write_all(out.as_bytes()).await?;
        match resp.truncate_at {
            Some(n) => {
                let n = (n as usize).min(resp.body.len());
                writer.write_all(&resp.body[..n]).await?;
                // writer 随即 drop 断开，模拟中途断连
            }
            None => writer.write_all(&resp.body).await?,
        }
        writer.flush().await?;
        Ok(())
    }

    /// 跑完一个任务，收集全部事件，返回 (是否成功, 失败原因, 事件列表)。
    async fn run_to_end(spec: TaskSpec) -> (bool, String, Vec<TaskEvent>) {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let ctrl = TaskControl::new();
        let handle = spawn_task(spec, ctrl, tx);
        let mut events = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            assert!(Instant::now() < deadline, "集成测试超时");
            let wait = Duration::from_secs(1).min(deadline.saturating_duration_since(Instant::now()));
            match tokio::time::timeout(wait, rx.recv()).await {
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
        handle.abort();
        match events.last() {
            Some(TaskEvent::Finished { ok, reason, .. }) => (*ok, reason.clone(), events),
            _ => panic!("缺少 Finished 事件"),
        }
    }

    fn tmp_file(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("vdl-it-{}-{tag}.bin", std::process::id()));
        let _ = fs::remove_file(&p);
        p
    }

    fn meta_resp(total: u64, ranges: bool) -> FakeResp {
        let mut headers = vec![
            ("Content-Length", total.to_string()),
            ("ETag", "\"m\"".into()),
        ];
        if ranges {
            headers.push(("Accept-Ranges", "bytes".into()));
        }
        FakeResp {
            status: 200,
            reason: "OK",
            headers,
            body: Vec::new(),
            truncate_at: None,
        }
    }

    fn spec_of(url: String, path: &Path) -> TaskSpec {
        TaskSpec {
            url,
            mirrors: Vec::new(),
            alt_url: None,
            save_path: path.to_path_buf(),
            proxy: None,
            segments: 8,
            timeout_seconds: 20,
            limits: SpeedLimits::default(),
        }
    }

    #[tokio::test]
    async fn it_speed_limit_no_read_timeout() {
        // 192KB @ 64KB/s：预期 ≥2s；验证限速等待不误触 30s 读超时（成功、无「超时」日志）
        let body = test_body(192 * 1024);
        let total = body.len() as u64;
        let body_srv = body.clone();
        let url = serve(2, move |i, req| match i {
            0 if req.method == "HEAD" => meta_resp(total, true),
            _ => FakeResp::ok(
                vec![("Content-Length", total.to_string())],
                body_srv.clone(),
            ),
        })
        .await;
        let path = tmp_file("speed-limit");
        let mut spec = spec_of(url, &path);
        spec.limits = SpeedLimits {
            global: Some(std::sync::Arc::new(Limiter::new(64_000))),
            task: None,
        };
        let start = Instant::now();
        let (ok, reason, events) = run_to_end(spec).await;
        let elapsed = start.elapsed();
        assert!(ok, "限速下载失败: {reason}");
        assert!(elapsed >= Duration::from_millis(1500), "限速未生效: {elapsed:?}");
        assert!(
            !events.iter().any(|e| matches!(
                e,
                TaskEvent::Log(LogLevel::Warn, m) if m.contains("超时")
            )),
            "限速等待不应产生读超时日志"
        );
        assert_eq!(fs::read(&path).unwrap(), body);
        let _ = fs::remove_file(&path);
    }

    #[tokio::test]
    async fn it_single_stream_full_download() {
        let body = test_body(1024 * 1024);
        let total = body.len() as u64;
        let body_srv = body.clone();
        let url = serve(2, move |i, req| match i {
            0 if req.method == "HEAD" => meta_resp(total, true),
            _ => FakeResp::ok(
                vec![("Content-Length", total.to_string()), ("ETag", "\"m\"".into())],
                body_srv.clone(),
            ),
        })
        .await;
        let path = tmp_file("single-full");
        let (ok, reason, _) = run_to_end(spec_of(url, &path)).await;
        assert!(ok, "下载失败: {reason}");
        assert_eq!(fs::read(&path).unwrap(), body);
        let _ = fs::remove_file(&path);
    }

    #[tokio::test]
    async fn it_single_stream_resume_after_cut() {
        let body = test_body(2 * 1024 * 1024);
        let total = body.len() as u64;
        let cut = total / 4;
        let body_for_srv = body.clone();
        let url = serve(3, move |i, req| match i {
            0 if req.method == "HEAD" => meta_resp(total, true),
            1 => {
                // 第一次下载：发 1/4 后断连
                let mut r = FakeResp::ok(
                    vec![("Content-Length", total.to_string()), ("ETag", "\"m\"".into())],
                    body_for_srv.clone(),
                );
                r.truncate_at = Some(cut);
                r
            }
            _ => {
                // 续传请求：必须带 Range，从断点继续
                let range = req.range.as_deref().expect("第二次请求应携带 Range 头");
                let s = range.trim_start_matches("bytes=");
                let start: u64 = s.split('-').next().unwrap().parse().unwrap();
                assert_eq!(start, cut, "应从断点 {cut} 续传");
                let headers = vec![
                    ("Content-Range", format!("bytes {start}-{}/{total}", total - 1)),
                    ("Content-Length", (total - start).to_string()),
                ];
                FakeResp {
                    status: 206,
                    reason: "Partial Content",
                    headers,
                    body: body_for_srv[start as usize..].to_vec(),
                    truncate_at: None,
                }
            }
        })
        .await;
        let path = tmp_file("single-resume");
        let (ok, reason, _) = run_to_end(spec_of(url, &path)).await;
        assert!(ok, "下载失败: {reason}");
        assert_eq!(fs::read(&path).unwrap(), body);
        let _ = fs::remove_file(&path);
    }

    #[tokio::test]
    async fn it_complete_file_416_treated_as_done() {
        let body = test_body(512 * 1024);
        let total = body.len() as u64;
        let body_srv = body.clone();
        let url = serve(3, move |i, req| match i {
            0 if req.method == "HEAD" => meta_resp(total, true),
            1 => {
                assert!(req.range.is_some(), "已有完整文件时应发送 Range 探测");
                FakeResp {
                    status: 416,
                    reason: "Range Not Satisfiable",
                    headers: vec![],
                    body: Vec::new(),
                    truncate_at: None,
                }
            }
            _ => meta_resp(total, true), // head_size 比对
        })
        .await;
        let path = tmp_file("complete-416");
        fs::write(&path, &body_srv).unwrap();
        let (ok, reason, events) = run_to_end(spec_of(url, &path)).await;
        assert!(ok, "应判定文件已完整: {reason}");
        assert!(
            events.iter().any(|e| matches!(
                e,
                TaskEvent::Log(LogLevel::Success, msg) if msg.contains("文件已完整")
            )),
            "应有「文件已完整」日志"
        );
        assert_eq!(fs::read(&path).unwrap(), body);
        let _ = fs::remove_file(&path);
    }

    #[tokio::test]
    async fn it_200_restart_when_range_ignored() {
        let body = test_body(1024 * 1024);
        let total = body.len() as u64;
        let body_srv = body.clone();
        let url = serve(3, move |i, req| match i {
            0 if req.method == "HEAD" => meta_resp(total, false), // 不宣告 Accept-Ranges
            _ => FakeResp::ok(
                vec![("Content-Length", total.to_string())],
                body_srv.clone(),
            ),
        })
        .await;
        let path = tmp_file("range-ignored");
        fs::write(&path, &body[..total as usize / 2]).unwrap(); // 半个旧文件
        let (ok, reason, events) = run_to_end(spec_of(url, &path)).await;
        assert!(ok, "下载失败: {reason}");
        assert!(
            events.iter().any(|e| matches!(
                e,
                TaskEvent::Log(LogLevel::Warn, msg) if msg.contains("不支持断点续传")
            )),
            "应有「不支持断点续传」日志"
        );
        assert_eq!(fs::read(&path).unwrap(), body);
        let _ = fs::remove_file(&path);
    }

    #[tokio::test]
    async fn it_segmented_download_and_assemble() {
        let body = test_body(9 * 1024 * 1024); // ≥ 8MiB 触发分段
        let total = body.len() as u64;
        let body_srv = body.clone();
        let url = serve(9, move |i, req| match i {
            0 if req.method == "HEAD" => meta_resp(total, true),
            _ => {
                let range = req.range.as_deref().expect("分段请求必须带 Range");
                let s = range.trim_start_matches("bytes=");
                let (a, b) = s.split_once('-').unwrap();
                let (start, end): (u64, u64) = (a.parse().unwrap(), b.parse().unwrap());
                FakeResp::partial(total, &body_srv, start, end)
            }
        })
        .await;
        let path = tmp_file("segmented");
        let (ok, reason, _) = run_to_end(spec_of(url, &path)).await;
        assert!(ok, "分段下载失败: {reason}");
        assert_eq!(fs::read(&path).unwrap(), body, "合并结果应与源一致");
        assert!(!part_path(&path, 0).exists(), "part 文件应被清理");
        assert!(!sidecar_path(&path).exists(), "sidecar 应被清理");
        let _ = fs::remove_file(&path);
    }

    #[tokio::test]
    async fn it_segmented_resume_after_cancel() {
        let body = test_body(9 * 1024 * 1024);
        let total = body.len() as u64;
        let seg_len = total / 8;

        // 同一个服务器服务两个阶段（续传场景 URL 不变，sidecar 才能匹配）：
        // 对分段 1 的第一次 GET 只回 1024 字节就断连，制造未完成段
        let body_srv = body.clone();
        let poisoned = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let poison_srv = poisoned.clone();
        let url = serve(30, move |_i, req| {
            if req.method == "HEAD" {
                return meta_resp(total, true);
            }
            let range = req.range.clone().unwrap_or_else(|| "bytes=0-0".to_string());
            let s = range.trim_start_matches("bytes=");
            let (a, b) = s.split_once('-').unwrap();
            let (start, end): (u64, u64) = (a.parse().unwrap(), b.parse().unwrap());
            if start == seg_len && !poison_srv.swap(true, Ordering::Relaxed) {
                let mut r = FakeResp::partial(total, &body_srv, start, end);
                r.truncate_at = Some(1024);
                return r;
            }
            FakeResp::partial(total, &body_srv, start, end)
        })
        .await;
        let path = tmp_file("segmented-resume");
        {
            let (tx, mut rx) = mpsc::unbounded_channel();
            let ctrl = TaskControl::new();
            let handle = spawn_task(
                TaskSpec {
                    url: url.clone(),
                    mirrors: Vec::new(),
                    alt_url: None,
                    save_path: path.clone(),
                    proxy: None,
                    segments: 8,
                    timeout_seconds: 20,
                    limits: SpeedLimits::default(),
                },
                ctrl.clone(),
                tx,
            );
            // 等到至少一个分段完整落盘再取消，确保续传能识别已完成分段
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                assert!(Instant::now() < deadline, "阶段 A 超时");
                if (0..8).any(|i| part_len(&path, i) >= seg_len) {
                    ctrl.stop();
                    break;
                }
                while let Ok(_e) = rx.try_recv() {}
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            loop {
                match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await {
                    Ok(Some(TaskEvent::Finished { .. })) => break,
                    Ok(Some(_)) => {}
                    Ok(None) => panic!("阶段 A 未收到结束事件"),
                    Err(_) => panic!("阶段 A 等待结束超时"),
                }
            }
            handle.abort();
        }
        let parts_left: usize = (0..8).filter(|&i| part_len(&path, i) > 0).count();
        assert!(parts_left > 0, "取消后应保留分段文件");
        assert!(sidecar_path(&path).exists(), "取消后应保留 sidecar");

        // 阶段 B：同一 URL 续传完成
        let (ok, reason, events) = run_to_end(spec_of(url, &path)).await;
        assert!(ok, "续传下载失败: {reason}");
        assert_eq!(fs::read(&path).unwrap(), body, "续传合并结果应与源一致");
        assert!(
            events.iter().any(|e| matches!(
                e,
                TaskEvent::Log(LogLevel::Info, msg) if msg.contains("段已完成")
            )),
            "应识别到已完成分段并续传"
        );
        assert!(!part_path(&path, 0).exists());
        let _ = fs::remove_file(&path);
    }

    #[tokio::test]
    async fn it_alt_url_fallback_on_404() {
        let body = test_body(256 * 1024);
        let total = body.len() as u64;
        let body_srv = body.clone();
        let base = serve(3, move |i, _req| match i {
            0 => FakeResp {
                // 主地址 HEAD → 404
                status: 404,
                reason: "Not Found",
                headers: vec![],
                body: Vec::new(),
                truncate_at: None,
            },
            1 => meta_resp(total, true), // 备用地址 HEAD
            _ => FakeResp::ok(
                vec![("Content-Length", total.to_string())],
                body_srv.clone(),
            ),
        })
        .await;
        let path = tmp_file("alt-404");
        let spec = TaskSpec {
            url: format!("{base}/main.zip"),
            alt_url: Some(format!("{base}/master.zip")),
            ..spec_of(format!("{base}/main.zip"), &path)
        };
        let (ok, reason, _) = run_to_end(spec).await;
        assert!(ok, "备用地址兜底失败: {reason}");
        assert_eq!(fs::read(&path).unwrap(), body);
        let _ = fs::remove_file(&path);
    }

    // ────────────── 多源聚合（v3 C5） ──────────────

    /// 从 Range 头解析 (start, end)。
    fn parse_range(range: &str) -> (u64, u64) {
        let s = range.trim_start_matches("bytes=");
        let (a, b) = s.split_once('-').unwrap();
        (a.parse().unwrap(), b.parse().unwrap())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn it_dual_source_tilts_to_faster() {
        let body = test_body(9 * 1024 * 1024); // 多源模式段数 = 8×2 = 16
        let total = body.len() as u64;
        let fast_gets = Arc::new(AtomicUsize::new(0));
        let slow_gets = Arc::new(AtomicUsize::new(0));

        let b1 = body.clone();
        let c1 = fast_gets.clone();
        let fast = serve(
            40,
            move |i, req| {
                if i == 0 && req.method == "HEAD" {
                    return meta_resp(total, true);
                }
                c1.fetch_add(1, Ordering::Relaxed);
                let (start, end) = parse_range(req.range.as_deref().expect("必须带 Range"));
                FakeResp::partial(total, &b1, start, end)
            },
        )
        .await;
        let b2 = body.clone();
        let c2 = slow_gets.clone();
        let slow = serve(
            40,
            move |i, req| {
                if i == 0 && req.method == "HEAD" {
                    return meta_resp(total, true);
                }
                c2.fetch_add(1, Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(150)); // 慢源：EMA 学习后调度器避开
                let (start, end) = parse_range(req.range.as_deref().expect("必须带 Range"));
                FakeResp::partial(total, &b2, start, end)
            },
        )
        .await;

        let path = tmp_file("dual-tilt");
        let mut spec = spec_of(fast, &path);
        spec.mirrors = vec![slow];
        let (ok, reason, _) = run_to_end(spec).await;
        assert!(ok, "双源下载失败: {reason}");
        let f = fast_gets.load(Ordering::Relaxed);
        let s = slow_gets.load(Ordering::Relaxed);
        assert!(f > s, "分段应向快源倾斜: fast={f} slow={s}");
        assert_eq!(fs::read(&path).unwrap(), body, "合并结果应与源一致");
        let _ = fs::remove_file(&path);
    }

    #[tokio::test]
    async fn it_source_death_reassigns_and_completes() {
        let body = test_body(9 * 1024 * 1024);
        let total = body.len() as u64;
        // 濒死源：元数据合法（能通过探测与大小校验），但 GET 一律 500
        let dying = serve(12, move |i, _req| match i {
            0 => meta_resp(total, true),
            _ => FakeResp {
                status: 500,
                reason: "Internal Server Error",
                headers: vec![],
                body: Vec::new(),
                truncate_at: None,
            },
        })
        .await;
        let body_srv = body.clone();
        let good = serve(
            40,
            move |i, req| match i {
                0 if req.method == "HEAD" => meta_resp(total, true),
                _ => {
                    let (start, end) = parse_range(req.range.as_deref().expect("必须带 Range"));
                    FakeResp::partial(total, &body_srv, start, end)
                }
            },
        )
        .await;

        let path = tmp_file("source-death");
        let mut spec = spec_of(good, &path);
        spec.mirrors = vec![dying];
        let (ok, reason, _) = run_to_end(spec).await;
        assert!(ok, "源死亡后应改派他源完成: {reason}");
        assert_eq!(fs::read(&path).unwrap(), body);
        let _ = fs::remove_file(&path);
    }

    #[tokio::test]
    async fn it_size_mismatched_mirror_rejected() {
        let body = test_body(9 * 1024 * 1024); // ≥ 8MiB 走分段路径
        let total = body.len() as u64;
        // 镜像源宣告大小不符 → 探测阶段剔除，GET 不会到达它（n=1 只预算 HEAD）
        let bad = serve(1, move |_i, _req| FakeResp {
            status: 200,
            reason: "OK",
            headers: vec![
                ("Content-Length", (total + 1000).to_string()),
                ("Accept-Ranges", "bytes".into()),
            ],
            body: Vec::new(),
            truncate_at: None,
        })
        .await;
        let body_srv = body.clone();
        let good = serve(
            20,
            move |i, req| match i {
                0 if req.method == "HEAD" => meta_resp(total, true),
                _ => {
                    let (start, end) = parse_range(req.range.as_deref().expect("必须带 Range"));
                    FakeResp::partial(total, &body_srv, start, end)
                }
            },
        )
        .await;

        let path = tmp_file("size-mismatch");
        let mut spec = spec_of(good, &path);
        spec.mirrors = vec![bad];
        let (ok, reason, events) = run_to_end(spec).await;
        assert!(ok, "大小不符源剔除后应正常完成: {reason}");
        assert!(
            events.iter().any(|e| matches!(
                e,
                TaskEvent::Log(LogLevel::Warn, m) if m.contains("大小不一致")
            )),
            "应有「大小不一致」剔除日志"
        );
        assert_eq!(fs::read(&path).unwrap(), body);
        let _ = fs::remove_file(&path);
    }

    #[tokio::test]
    async fn it_multi_source_resume_after_cancel() {
        let body = test_body(9 * 1024 * 1024);
        let total = body.len() as u64;
    let seg_count = 16usize; // 多源：8 线程 × 2
    let seg_len = total / seg_count as u64;

        let b1 = body.clone();
        let s1 = serve(
            40,
            move |_i, req| {
                // 按 method 判断（阶段 B 的 HEAD 排在阶段 A 之后，不能用连接序号）
                if req.method == "HEAD" {
                    return meta_resp(total, true);
                }
                let (start, end) = parse_range(req.range.as_deref().expect("必须带 Range"));
                FakeResp::partial(total, &b1, start, end)
            },
        )
        .await;
        let b2 = body.clone();
        let s2 = serve(
            40,
            move |_i, req| {
                if req.method == "HEAD" {
                    return meta_resp(total, true);
                }
                let (start, end) = parse_range(req.range.as_deref().expect("必须带 Range"));
                FakeResp::partial(total, &b2, start, end)
            },
        )
        .await;

        let path = tmp_file("multi-resume");
        let mk_spec = || TaskSpec {
            url: s1.clone(),
            mirrors: vec![s2.clone()],
            alt_url: None,
            save_path: path.clone(),
            proxy: None,
            segments: 8,
            timeout_seconds: 20,
            limits: SpeedLimits::default(),
        };
        // 阶段 A：任一分段完整落盘后取消
        {
            let (tx, mut rx) = mpsc::unbounded_channel();
            let ctrl = TaskControl::new();
            let handle = spawn_task(mk_spec(), ctrl.clone(), tx);
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                assert!(Instant::now() < deadline, "阶段 A 超时");
                if (0..seg_count).any(|i| part_len(&path, i) >= seg_len) {
                    ctrl.stop();
                    break;
                }
                while let Ok(_e) = rx.try_recv() {}
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            loop {
                match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await {
                    Ok(Some(TaskEvent::Finished { .. })) => break,
                    Ok(Some(_)) => {}
                    Ok(None) => panic!("阶段 A 未收到结束事件"),
                    Err(_) => panic!("阶段 A 等待结束超时"),
                }
            }
            handle.abort();
        }
        assert!(
            (0..seg_count).any(|i| part_len(&path, i) > 0),
            "取消后应保留分段文件"
        );
        assert!(sidecar_path(&path).exists(), "取消后应保留 sidecar");

        // 阶段 B：同源续传完成
        let (ok, reason, events) = run_to_end(mk_spec()).await;
        assert!(ok, "多源续传失败: {reason}");
        assert_eq!(fs::read(&path).unwrap(), body, "续传合并结果应与源一致");
        assert!(
            events.iter().any(|e| matches!(
                e,
                TaskEvent::Log(LogLevel::Info, m) if m.contains("段已完成")
            )),
            "应识别到已完成分段并续传"
        );
        let _ = fs::remove_file(&path);
    }
}
