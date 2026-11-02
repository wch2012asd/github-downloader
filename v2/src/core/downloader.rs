//! 下载核心：探测 → 规划 → 单流/分段下载 → 合并，断点续传与自动重试。
//!
//! 并发模型：每个任务一个编排线程；分段模式下由编排线程派发 N 个分段线程，
//! 各分段独立写 `.part{i}` 文件（文件长度即进度，崩溃后可恢复）。
//! 暂停/恢复/停止通过 [`TaskControl`] 在 chunk 间隙生效。

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::core::url_parser::percent_decode;
use crate::events::{LogLevel, Progress, UiEvent};

pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) GitHubDownloader/2.0";

const MAX_ATTEMPTS: u32 = 8; // 首次下载 + 最多 7 次自动重试（对齐 v1）
const RETRY_DELAY: Duration = Duration::from_secs(5);
const CHUNK: usize = 64 * 1024;
const TICK: Duration = Duration::from_millis(500);
/// 两次数据之间最多等待的时长（v1 的 read timeout 语义）；超时按可重试中断处理
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// 总大小低于该阈值时退回单流（小文件分段得不偿失）
const SEGMENT_MIN_SIZE: u64 = 8 * 1024 * 1024;
/// 每段最小跨度，避免碎片段
const SEGMENT_MIN_SPAN: u64 = 1024 * 1024;
const MAX_SEGMENTS: u8 = 32;

/// 下载前探测到的资源元数据。
#[derive(Debug, Clone, Default)]
pub struct Probe {
    pub total: Option<u64>,
    pub accept_ranges: bool,
    pub etag: Option<String>,
    pub filename: Option<String>,
    /// 响应 Content-Type（用于提示“链接返回的是网页而非文件”）
    pub content_type: Option<String>,
}

/// 任务控制状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ctrl {
    Running,
    Paused,
    Stopping,
}

/// 跨线程共享的暂停/恢复/停止控制。
#[derive(Clone)]
pub struct TaskControl(Arc<(Mutex<Ctrl>, Condvar)>);

impl Default for TaskControl {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskControl {
    pub fn new() -> Self {
        Self(Arc::new((Mutex::new(Ctrl::Running), Condvar::new())))
    }

    fn set(&self, c: Ctrl) {
        let (m, cv) = &*self.0;
        *m.lock().unwrap() = c;
        cv.notify_all();
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
        *self.0 .0.lock().unwrap()
    }

    /// 阻塞直到任务恢复运行；返回 false 表示已请求停止。
    pub fn wait_running(&self) -> bool {
        let (m, cv) = &*self.0;
        let mut guard = m.lock().unwrap();
        loop {
            match *guard {
                Ctrl::Paused => guard = cv.wait(guard).unwrap(),
                Ctrl::Running => return true,
                Ctrl::Stopping => return false,
            }
        }
    }
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
    /// 主地址 404 时的备用地址（仓库主页 main.zip → master.zip 兜底）
    pub alt_url: Option<String>,
    pub save_path: PathBuf,
    /// Some("http://127.0.0.1:port")，仅 clash 通道
    pub proxy: Option<String>,
    pub segments: u8,
    /// 连接与探测超时（秒），来自设置的「网络连接请求超时」
    pub timeout_seconds: u32,
}

#[derive(Debug)]
pub enum TaskFail {
    Cancelled,
    Failed(String),
}

/// 启动下载任务线程；结束时发送 `TaskFinished`。
pub fn spawn_task(
    task_gen: u64,
    spec: TaskSpec,
    ctrl: TaskControl,
    tx: Sender<UiEvent>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let result = run(task_gen, &spec, &ctrl, &tx);
        let reason = match &result {
            Ok(()) => String::new(),
            Err(TaskFail::Cancelled) => "已取消".into(),
            Err(TaskFail::Failed(r)) => r.clone(),
        };
        let _ = tx.send(UiEvent::TaskFinished {
            task_gen,
            ok: result.is_ok(),
            reason,
        });
    })
}

fn run(
    task_gen: u64,
    spec: &TaskSpec,
    ctrl: &TaskControl,
    tx: &Sender<UiEvent>,
) -> Result<(), TaskFail> {
    let log = |level: LogLevel, msg: String| {
        let _ = tx.send(UiEvent::Log { level, msg });
    };
    let timeout = Duration::from_secs(spec.timeout_seconds.max(5) as u64);
    let client = build_client(spec.proxy.as_deref(), timeout)
        .map_err(|e| TaskFail::Failed(format!("网络客户端创建失败: {e}")))?;

    // 探测：先主地址，404 且有备用地址（仓库主页 master 兜底）时换备用
    let mut url = spec.url.clone();
    let probe = match probe_resource(&client, &url, timeout) {
        Ok(p) => p,
        Err(ProbeFail::NotFound) => {
            if let Some(alt) = spec.alt_url.as_deref() {
                log(LogLevel::Warn, "请求的资源不存在（404），尝试备用地址…".into());
                url = alt.to_string();
                match probe_resource(&client, &url, timeout) {
                    Ok(p) => p,
                    Err(_) => return Err(TaskFail::Failed("下载失败: HTTP 404".into())),
                }
            } else {
                return Err(TaskFail::Failed("下载失败: HTTP 404".into()));
            }
        }
        Err(ProbeFail::Unreachable) => {
            log(LogLevel::Warn, "资源探测失败，将按未知大小直接下载".into());
            Probe::default()
        }
    };
    if let Some(name) = &probe.filename {
        log(LogLevel::Info, format!("服务器返回文件名: {name}"));
    }

    // 许可门槛（如 SEGGER）：响应为网页时，解析页面中的许可确认表单并自动提交
    let gate = if probe
        .content_type
        .as_deref()
        .is_some_and(|ct| ct.starts_with("text/html"))
    {
        log(LogLevel::Info, "该站点需要确认使用条款，正在自动处理…".to_string());
        match resolve_license_gate(&client, &url) {
            Ok(Some(g)) => {
                log(LogLevel::Success, "已自动接受站点使用条款，开始下载".to_string());
                Some(g)
            }
            Ok(None) => {
                log(
                    LogLevel::Error,
                    "链接返回的是网页且未识别到许可确认表单，无法自动下载（请用「直链下载」在浏览器中完成）"
                        .to_string(),
                );
                return Err(TaskFail::Failed("链接返回的是网页而非直接文件".into()));
            }
            Err(e) => {
                log(LogLevel::Warn, format!("许可确认处理失败: {e}"));
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
        run_segmented(
            &client,
            &url,
            &spec.save_path,
            &probe,
            total,
            n as u8,
            ctrl,
            tx,
            task_gen,
            gate.as_ref(),
        )
    } else {
        run_single(
            &client,
            &url,
            &spec.save_path,
            probe.total,
            ctrl,
            tx,
            task_gen,
            gate.as_ref(),
        )
    }
}

// ────────────────────────── HTTP 客户端与探测 ──────────────────────────

fn build_client(
    proxy: Option<&str>,
    connect_timeout: Duration,
) -> Result<reqwest::blocking::Client, reqwest::Error> {
    // 注意：blocking ClientBuilder 没有 read_timeout，整体 timeout 会误杀长下载；
    // 读间隔超时由 StallReader 实现
    let mut builder = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(connect_timeout);
    if let Some(p) = proxy {
        builder = builder.proxy(reqwest::Proxy::all(p)?);
    }
    builder.build()
}

/// 单次读取结果。
enum ReadOutcome {
    Data(Vec<u8>),
    Eof,
    Timeout,
    Err(std::io::Error),
}

/// 在独立线程阻塞读响应、任务线程带超时等待，从而实现 v1 的
/// 「两次数据之间最多等 30s」语义（blocking reqwest 无 read_timeout）。
/// 超时后读线程被放弃（线程与连接随进程退出回收），上层按可重试中断处理。
struct StallReader {
    rx: std::sync::mpsc::Receiver<ReadOutcome>,
    thread: Option<JoinHandle<()>>,
}

impl StallReader {
    fn start(resp: reqwest::blocking::Response) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let thread = thread::spawn(move || {
            let mut resp = resp;
            let mut buf = [0u8; CHUNK];
            loop {
                match resp.read(&mut buf) {
                    Ok(0) => {
                        let _ = tx.send(ReadOutcome::Eof);
                        break;
                    }
                    Ok(n) => {
                        if tx.send(ReadOutcome::Data(buf[..n].to_vec())).is_err() {
                            break; // 接收端已超时放弃
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        let _ = tx.send(ReadOutcome::Err(e));
                        break;
                    }
                }
            }
        });
        Self {
            rx,
            thread: Some(thread),
        }
    }

    fn read_chunk(&mut self) -> ReadOutcome {
        match self.rx.recv_timeout(READ_TIMEOUT) {
            Ok(outcome) => outcome,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                self.thread.take(); // 分离阻塞中的读线程
                ReadOutcome::Timeout
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => ReadOutcome::Eof,
        }
    }
}

enum ProbeFail {
    NotFound,
    Unreachable,
}

/// 探测资源大小/Range 支持/ETag：HEAD 优先，被拒或不可达时退化为 `GET Range: bytes=0-0`。
fn probe_resource(
    client: &reqwest::blocking::Client,
    url: &str,
    timeout: Duration,
) -> Result<Probe, ProbeFail> {
    if let Ok(resp) = client.head(url).timeout(timeout).send() {
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

fn header_u64(headers: &reqwest::header::HeaderMap, name: reqwest::header::HeaderName) -> Option<u64> {
    headers
        .get(name)?
        .to_str()
        .ok()?
        .parse()
        .ok()
}

/// `Content-Range: bytes 0-0/1234` → 1234。
fn parse_content_range_total(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    let v = headers
        .get(reqwest::header::CONTENT_RANGE)?
        .to_str()
        .ok()?;
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
pub fn fetch_size(url: &str, proxy: Option<&str>, timeout_seconds: u32) -> Option<u64> {
    let timeout = Duration::from_secs(timeout_seconds.max(5) as u64);
    let client = build_client(proxy, timeout).ok()?;
    let resp = client.head(url).timeout(timeout).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    header_u64(resp.headers(), reqwest::header::CONTENT_LENGTH)
}

// ────────────────────────── 单流下载（对齐 v1 行为） ──────────────────────────

fn run_single(
    client: &reqwest::blocking::Client,
    url: &str,
    save_path: &Path,
    known_total: Option<u64>,
    ctrl: &TaskControl,
    tx: &Sender<UiEvent>,
    task_gen: u64,
    gate: Option<&GateForm>,
) -> Result<(), TaskFail> {
    let log = |level: LogLevel, msg: String| {
        let _ = tx.send(UiEvent::Log { level, msg });
    };
    let progress = |p: Progress| {
        let _ = tx.send(UiEvent::Progress { task_gen, progress: p });
    };

    for attempt in 1..=MAX_ATTEMPTS {
        if !ctrl.wait_running() {
            log(LogLevel::Warn, "下载已取消".into());
            return Err(TaskFail::Cancelled);
        }

        let existing = fs::metadata(save_path).map(|m| m.len()).unwrap_or(0);
        let range = (existing > 0).then(|| format!("bytes={existing}-"));
        let mut req = build_request(client, gate, url, range);
        if existing > 0 {
            req = req.header("Range", format!("bytes={existing}-"));
            log(
                LogLevel::Info,
                format!("检测到已有文件，从 {existing} 字节处断点续传"),
            );
        }
        let resp = match req.send() {
            Ok(r) => r,
            Err(e) => {
                log(LogLevel::Error, format!("连接失败: {e}"));
                match wait_retry(attempt, ctrl, &log) {
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
                    log(LogLevel::Warn, "服务器不支持断点续传，重新下载".into());
                }
                (content_length_of(&resp).or(known_total), false)
            }
            416 => {
                let remote = head_size(client, url);
                if let Some(r) = remote {
                    if existing >= r {
                        log(
                            LogLevel::Success,
                            format!("文件已完整，无需下载 ({existing} 字节)"),
                        );
                        progress(Progress {
                            downloaded: existing,
                            total: existing,
                            speed_bps: 0.0,
                            pct: 100.0,
                        });
                        return Ok(());
                    }
                }
                log(
                    LogLevel::Error,
                    format!("服务器拒绝续传 (416)，本地文件 ({existing} 字节) 与服务器不一致，请删除后重试"),
                );
                return Err(TaskFail::Failed(format!(
                    "HTTP 416（本地 {existing} 字节与服务器不一致）"
                )));
            }
            s if (500..600).contains(&s) => {
                log(LogLevel::Error, format!("连接失败: HTTP {s}"));
                match wait_retry(attempt, ctrl, &log) {
                    Retry::Next => continue,
                    Retry::Cancelled => return Err(TaskFail::Cancelled),
                    Retry::Exhausted => return Err(TaskFail::Failed("自动重试次数用尽".into())),
                }
            }
            s => {
                log(LogLevel::Error, format!("连接失败: HTTP {s}"));
                return Err(TaskFail::Failed(format!("HTTP {s}")));
            }
        };
        let total = total.unwrap_or(0);

        let mut file = match open_for_write(save_path, append) {
            Ok(f) => f,
            Err(e) => {
                log(LogLevel::Error, format!("下载出错: 无法打开文件: {e}"));
                return Err(TaskFail::Failed(format!("无法写入文件: {e}")));
            }
        };

        let mut downloaded = if append { existing } else { 0 };
        let mut reader = StallReader::start(resp);
        let mut last_tick = Instant::now();
        let mut last_bytes = downloaded;
        let mut interrupted: Option<String> = None;

        loop {
            if !ctrl.wait_running() {
                log(LogLevel::Warn, "下载已取消".into());
                return Err(TaskFail::Cancelled);
            }
            match reader.read_chunk() {
                ReadOutcome::Eof => break,
                ReadOutcome::Data(data) => {
                    if let Err(e) = file.write_all(&data) {
                        log(LogLevel::Error, format!("下载出错: 写入失败: {e}"));
                        return Err(TaskFail::Failed(format!("写入文件失败: {e}")));
                    }
                    downloaded += data.len() as u64;
                    if last_tick.elapsed() >= TICK {
                        let dt = last_tick.elapsed().as_secs_f64();
                        let speed = (downloaded - last_bytes) as f64 / dt.max(0.001);
                        progress(Progress {
                            downloaded,
                            total,
                            speed_bps: speed,
                            pct: pct_of(downloaded, total),
                        });
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
                progress(Progress {
                    downloaded,
                    total: total.max(downloaded),
                    speed_bps: 0.0,
                    pct,
                });
                log(
                    LogLevel::Success,
                    format!("下载完成: {} ({downloaded} 字节)", save_path.display()),
                );
                return Ok(());
            }
            Some(msg) => {
                // 已写入的部分保留，交给下一轮续传
                log(LogLevel::Warn, msg);
                match wait_retry(attempt, ctrl, &log) {
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
fn open_for_write(path: &Path, append: bool) -> std::io::Result<File> {
    if append {
        OpenOptions::new().create(true).append(true).open(path)
    } else {
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
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
    sibling_path(save_path, ".ghdl.json")
}

/// 清理分段残留（part 文件与 sidecar）。
fn cleanup_parts(save_path: &Path, segments: u8) {
    for i in 0..segments as usize {
        let _ = fs::remove_file(part_path(save_path, i));
    }
    let _ = fs::remove_file(sidecar_path(save_path));
}

enum SegFail {
    /// 暂停或停止导致的安全退出
    Stopped,
    /// 服务器忽略 Range 返回了整文件内容，分段不可行
    NoRange,
    Fatal(String),
}

#[allow(clippy::too_many_arguments)]
fn run_segmented(
    client: &reqwest::blocking::Client,
    url: &str,
    save_path: &Path,
    probe: &Probe,
    total: u64,
    segments: u8,
    ctrl: &TaskControl,
    tx: &Sender<UiEvent>,
    task_gen: u64,
    gate: Option<&GateForm>,
) -> Result<(), TaskFail> {
    let log = |level: LogLevel, msg: String| {
        let _ = tx.send(UiEvent::Log { level, msg });
    };
    let segs = plan_segments(total, segments);

    // 复用未完成的分段：sidecar 与本次任务一致才可信
    let sidecar: Option<Sidecar> = fs::read(sidecar_path(save_path))
        .ok()
        .and_then(|d| serde_json::from_slice(&d).ok());
    let reusable = sidecar.as_ref().is_some_and(|s| {
        s.url == url && s.total == total && s.segments == segments && s.etag == probe.etag
    });
    if !reusable {
        if (0..segments as usize).any(|i| part_len(save_path, i) > 0) {
            log(LogLevel::Warn, "已有分段文件与服务器不一致，重新下载".into());
        }
        cleanup_parts(save_path, segments);
        let sidecar = Sidecar {
            url: url.to_string(),
            etag: probe.etag.clone(),
            total,
            segments,
        };
        if let Ok(json) = serde_json::to_vec(&sidecar) {
            let _ = fs::write(sidecar_path(save_path), json);
        }
    } else {
        let done = (0..segments as usize)
            .filter(|&i| part_len(save_path, i) >= segs[i].len())
            .count();
        if done > 0 {
            log(
                LogLevel::Info,
                format!("检测到未完成的分段下载，其中 {done}/{segments} 段已完成"),
            );
        }
    }

    let mut ema_speed = 0.0f64;
    loop {
        if !ctrl.wait_running() {
            return Err(TaskFail::Cancelled);
        }
        let base: Vec<u64> = (0..segments as usize)
            .map(|i| part_len(save_path, i).min(segs[i].len()))
            .collect();
        let base_sum: u64 = base.iter().sum();
        let written = Arc::new(AtomicU64::new(0));

        let mut handles: Vec<JoinHandle<Result<(), SegFail>>> = Vec::new();
        for (i, seg) in segs.iter().enumerate() {
            if base[i] >= seg.len() {
                continue;
            }
            let client = client.clone();
            let url = url.to_string();
            let path = part_path(save_path, i);
            let seg = *seg;
            let ctrl = ctrl.clone();
            let written = written.clone();
            let gate_owned: Option<GateForm> = gate.cloned();
            let log_tx = tx.clone();
            handles.push(thread::spawn(move || {
                let log = |level: LogLevel, msg: String| {
                    let _ = log_tx.send(UiEvent::Log { level, msg });
                };
                run_segment(&client, &url, &path, seg, &ctrl, &written, gate_owned.as_ref(), log)
            }));
        }

        // 进度 tick：等全部分段线程结束
        let mut last_tick = Instant::now();
        let mut last_bytes = base_sum;
        loop {
            if handles.iter().all(|h| h.is_finished()) {
                break;
            }
            thread::sleep(Duration::from_millis(100));
            if last_tick.elapsed() >= TICK {
                let downloaded = base_sum + written.load(Ordering::Relaxed);
                let inst = (downloaded - last_bytes) as f64
                    / last_tick.elapsed().as_secs_f64().max(0.001);
                ema_speed = if ema_speed == 0.0 {
                    inst
                } else {
                    ema_speed * 0.6 + inst * 0.4
                };
                let _ = tx.send(UiEvent::Progress {
                    task_gen,
                    progress: Progress {
                        downloaded,
                        total,
                        speed_bps: ema_speed,
                        pct: pct_of(downloaded, total),
                    },
                });
                last_tick = Instant::now();
                last_bytes = downloaded;
            }
        }

        let mut fatal: Option<String> = None;
        let mut no_range = false;
        let mut stopped = false;
        for h in handles {
            match h.join() {
                Ok(Ok(())) => {}
                Ok(Err(SegFail::Stopped)) => stopped = true,
                Ok(Err(SegFail::NoRange)) => no_range = true,
                Ok(Err(SegFail::Fatal(e))) => fatal = Some(e),
                Err(_) => fatal = Some("分段线程异常退出".into()),
            }
        }
        if let Some(e) = fatal {
            return Err(TaskFail::Failed(e));
        }
        if no_range {
            log(LogLevel::Warn, "服务器不支持分段下载，退回单线程模式".into());
            cleanup_parts(save_path, segments);
            return run_single(client, url, save_path, Some(total), ctrl, tx, task_gen, gate);
        }
        if stopped {
            if ctrl.state() == Ctrl::Stopping {
                log(LogLevel::Warn, "下载已取消".into());
                return Err(TaskFail::Cancelled);
            }
            // Paused：阻塞等恢复
            if !ctrl.wait_running() {
                log(LogLevel::Warn, "下载已取消".into());
                return Err(TaskFail::Cancelled);
            }
            log(LogLevel::Info, "继续下载".into());
            continue;
        }
        // 全部段完成
        break;
    }

    // 合并
    let mut final_file = match File::create(save_path) {
        Ok(f) => f,
        Err(e) => return Err(TaskFail::Failed(format!("无法创建文件: {e}"))),
    };
    for (i, seg) in segs.iter().enumerate() {
        if part_len(save_path, i) != seg.len() {
            return Err(TaskFail::Failed("分段文件不完整，请重试".into()));
        }
        let mut part = match File::open(part_path(save_path, i)) {
            Ok(f) => f,
            Err(e) => return Err(TaskFail::Failed(format!("读取分段失败: {e}"))),
        };
        if let Err(e) = std::io::copy(&mut part, &mut final_file) {
            return Err(TaskFail::Failed(format!("合并分段失败: {e}")));
        }
        let _ = fs::remove_file(part_path(save_path, i));
    }
    let _ = fs::remove_file(sidecar_path(save_path));
    let _ = tx.send(UiEvent::Progress {
        task_gen,
        progress: Progress {
            downloaded: total,
            total,
            speed_bps: 0.0,
            pct: 100.0,
        },
    });
    log(
        LogLevel::Success,
        format!("下载完成: {} ({total} 字节)", save_path.display()),
    );
    Ok(())
}

fn run_segment(
    client: &reqwest::blocking::Client,
    url: &str,
    part: &Path,
    seg: Seg,
    ctrl: &TaskControl,
    written: &AtomicU64,
    gate: Option<&GateForm>,
    log: impl Fn(LogLevel, String),
) -> Result<(), SegFail> {
    let seg_len = seg.len();
    for attempt in 1..=MAX_ATTEMPTS {
        if !ctrl.wait_running() {
            return Err(SegFail::Stopped);
        }
        let existing = fs::metadata(part).map(|m| m.len()).unwrap_or(0);
        if existing > seg_len {
            return Err(SegFail::Fatal(format!(
                "分段文件超过预期大小（{} > {seg_len}）",
                part.display()
            )));
        }
        if existing == seg_len {
            return Ok(());
        }

        let resp = match build_request(
            client,
            gate,
            url,
            Some(format!("bytes={}-{}", seg.start + existing, seg.end)),
        )
        .send()
        {
            Ok(r) => r,
            Err(e) => {
                log(LogLevel::Warn, format!("分段连接失败: {e}"));
                match seg_wait_retry(attempt, ctrl, &log) {
                    SegRetry::Next => continue,
                    SegRetry::Stopped => return Err(SegFail::Stopped),
                    SegRetry::Exhausted => {
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
                log(LogLevel::Warn, format!("分段下载遇到 HTTP {s}"));
                match seg_wait_retry(attempt, ctrl, &log) {
                    SegRetry::Next => continue,
                    SegRetry::Stopped => return Err(SegFail::Stopped),
                    SegRetry::Exhausted => {
                        return Err(SegFail::Fatal("分段自动重试次数用尽".into()))
                    }
                }
            }
            s => return Err(SegFail::Fatal(format!("分段下载失败: HTTP {s}"))),
        }

        let mut file = match OpenOptions::new().create(true).append(true).open(part) {
            Ok(f) => f,
            Err(e) => return Err(SegFail::Fatal(format!("无法写入分段文件: {e}"))),
        };
        let mut reader = StallReader::start(resp);
        let mut done = existing;
        let mut interrupted = false;
        loop {
            if !ctrl.wait_running() {
                return Err(SegFail::Stopped);
            }
            match reader.read_chunk() {
                ReadOutcome::Eof => break,
                ReadOutcome::Data(data) => {
                    if let Err(e) = file.write_all(&data) {
                        return Err(SegFail::Fatal(format!("写入分段失败: {e}")));
                    }
                    done += data.len() as u64;
                    written.fetch_add(data.len() as u64, Ordering::Relaxed);
                    if done > seg_len {
                        return Err(SegFail::Fatal("服务器返回数据超出分段范围".into()));
                    }
                }
                ReadOutcome::Err(e) => {
                    log(LogLevel::Warn, format!("分段下载中断: {e}"));
                    interrupted = true;
                    break;
                }
                ReadOutcome::Timeout => {
                    log(LogLevel::Warn, "分段下载中断: 连接超时无响应".into());
                    interrupted = true;
                    break;
                }
            }
        }
        if !interrupted && done == seg_len {
            return Ok(());
        }
        if !interrupted {
            log(LogLevel::Warn, "分段连接提前关闭".into());
        }
        match seg_wait_retry(attempt, ctrl, &log) {
            SegRetry::Next => {}
            SegRetry::Stopped => return Err(SegFail::Stopped),
            SegRetry::Exhausted => return Err(SegFail::Fatal("分段自动重试次数用尽".into())),
        }
    }
    Err(SegFail::Fatal("分段自动重试次数用尽".into()))
}

// ────────────────────────── 许可门槛（SEGGER 类站点） ──────────────────────────

/// 按许可表单/断点需求构造下载请求。
fn build_request<'a>(
    client: &'a reqwest::blocking::Client,
    gate: Option<&GateForm>,
    url: &str,
    range: Option<String>,
) -> reqwest::blocking::RequestBuilder {
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
fn resolve_license_gate(
    client: &reqwest::blocking::Client,
    page_url: &str,
) -> Result<Option<GateForm>, String> {
    let resp = client
        .get(page_url)
        .timeout(Duration::from_secs(15))
        .send()
        .map_err(|e| format!("获取页面失败: {e}"))?;
    if resp.status().is_server_error() {
        return Err("站点返回 5xx".into());
    }
    let html = resp.text().map_err(|e| format!("读取页面失败: {e}"))?;
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
fn wait_retry(attempt: u32, ctrl: &TaskControl, log: &impl Fn(LogLevel, String)) -> Retry {
    match seg_wait_retry(attempt, ctrl, log) {
        SegRetry::Next => Retry::Next,
        SegRetry::Stopped => Retry::Cancelled,
        SegRetry::Exhausted => Retry::Exhausted,
    }
}

enum SegRetry {
    Next,
    Stopped,
    Exhausted,
}

fn seg_wait_retry(
    attempt: u32,
    ctrl: &TaskControl,
    log: &impl Fn(LogLevel, String),
) -> SegRetry {
    if attempt >= MAX_ATTEMPTS {
        return SegRetry::Exhausted;
    }
    log(
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
            return SegRetry::Stopped;
        }
        thread::sleep(Duration::from_millis(100));
    }
    SegRetry::Next
}

fn content_length_of(resp: &reqwest::blocking::Response) -> Option<u64> {
    header_u64(resp.headers(), reqwest::header::CONTENT_LENGTH)
}

fn head_size(client: &reqwest::blocking::Client, url: &str) -> Option<u64> {
    let resp = client.head(url).timeout(Duration::from_secs(15)).send().ok()?;
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

    #[test]
    fn control_pause_resume_stop() {
        let ctrl = TaskControl::new();
        assert_eq!(ctrl.state(), Ctrl::Running);
        ctrl.pause();
        assert_eq!(ctrl.state(), Ctrl::Paused);
        ctrl.resume();
        assert_eq!(ctrl.state(), Ctrl::Running);
        ctrl.stop();
        assert!(!ctrl.wait_running());
    }

    #[test]
    fn sibling_paths() {
        let p = Path::new("D:/dl/app.zip");
        assert_eq!(part_path(p, 2), Path::new("D:/dl/app.zip.part2"));
        assert_eq!(sidecar_path(p), Path::new("D:/dl/app.zip.ghdl.json"));
    }

    // ────────────── 集成测试：本地假 HTTP 服务器 ──────────────

    use std::io::{BufRead, BufReader};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc::Receiver;

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
                headers: vec![(
                    "Content-Range",
                    format!("bytes {start}-{end}/{total}"),
                )],
                body: full[start as usize..=(end as usize).min(full.len() - 1)].to_vec(),
                truncate_at: None,
            }
        }
    }

    fn test_body(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 7 % 251) as u8).collect()
    }

    /// 启动一个处理 n_requests 个请求的假服务器，返回基础 URL。
    fn serve<F>(n_requests: usize, handler: F) -> String
    where
        F: Fn(usize, &FakeReq) -> FakeResp + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定测试端口");
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for i in 0..n_requests {
                let Ok((stream, _)) = listener.accept() else {
                    break;
                };
                if let Err(e) = handle_fake_conn(stream, i, &handler) {
                    eprintln!("[fake-server] 连接处理失败: {e}");
                }
            }
        });
        format!("http://{addr}/file.bin")
    }

    fn handle_fake_conn<F>(stream: TcpStream, idx: usize, handler: &F) -> std::io::Result<()>
    where
        F: Fn(usize, &FakeReq) -> FakeResp,
    {
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut stream = stream;
        let mut first = String::new();
        reader.read_line(&mut first)?;
        let method = first
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string();
        let mut range = None;
        loop {
            let mut h = String::new();
            if reader.read_line(&mut h)? == 0 {
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
        stream.write_all(out.as_bytes())?;
        match resp.truncate_at {
            Some(n) => {
                let n = (n as usize).min(resp.body.len());
                stream.write_all(&resp.body[..n])?;
                // 直接 drop 断开，模拟中途断连
            }
            None => stream.write_all(&resp.body)?,
        }
        stream.flush()?;
        Ok(())
    }

    /// 跑完一个任务，收集全部事件，返回 (是否成功, 失败原因, 事件列表)。
    fn run_to_end(spec: TaskSpec) -> (bool, String, Vec<UiEvent>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let ctrl = TaskControl::new();
        let _handle = spawn_task(1, spec, ctrl, tx);
        collect_events(&rx)
    }

    fn collect_events(rx: &Receiver<UiEvent>) -> (bool, String, Vec<UiEvent>) {
        let mut events = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let remain = deadline.saturating_duration_since(Instant::now());
            assert!(!remain.is_zero(), "集成测试超时");
            match rx.recv_timeout(remain.min(Duration::from_secs(1))) {
                Ok(e) => {
                    let done = matches!(&e, UiEvent::TaskFinished { .. });
                    events.push(e);
                    if done {
                        break;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => panic!("事件通道关闭"),
            }
        }
        match events.last() {
            Some(UiEvent::TaskFinished { ok, reason, .. }) => (*ok, reason.clone(), events),
            _ => panic!("缺少 TaskFinished 事件"),
        }
    }

    fn tmp_file(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("ghdl-it-{}-{tag}.bin", std::process::id()));
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

    #[test]
    fn it_single_stream_full_download() {
        let body = test_body(1024 * 1024);
        let total = body.len() as u64;
        let body_srv = body.clone();
        let url = serve(2, move |i, req| match i {
            0 if req.method == "HEAD" => meta_resp(total, true),
            _ => FakeResp::ok(
                vec![("Content-Length", total.to_string()), ("ETag", "\"m\"".into())],
                body_srv.clone(),
            ),
        });
        let path = tmp_file("single-full");
        let (ok, reason, _) = run_to_end(TaskSpec {
            url,
            alt_url: None,
            save_path: path.clone(),
            proxy: None,
            segments: 8,
            timeout_seconds: 20,
        });
        assert!(ok, "下载失败: {reason}");
        assert_eq!(fs::read(&path).unwrap(), body);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn it_single_stream_resume_after_cut() {
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
        });
        let path = tmp_file("single-resume");
        let (ok, reason, _) = run_to_end(TaskSpec {
            url,
            alt_url: None,
            save_path: path.clone(),
            proxy: None,
            segments: 8,
            timeout_seconds: 20,
        });
        assert!(ok, "下载失败: {reason}");
        assert_eq!(fs::read(&path).unwrap(), body);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn it_complete_file_416_treated_as_done() {
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
        });
        let path = tmp_file("complete-416");
        fs::write(&path, &body_srv).unwrap();
        let (ok, reason, events) = run_to_end(TaskSpec {
            url,
            alt_url: None,
            save_path: path.clone(),
            proxy: None,
            segments: 8,
            timeout_seconds: 20,
        });
        assert!(ok, "应判定文件已完整: {reason}");
        assert!(
            events.iter().any(|e| matches!(
                e,
                UiEvent::Log { level: LogLevel::Success, msg } if msg.contains("文件已完整")
            )),
            "应有「文件已完整」日志"
        );
        assert_eq!(fs::read(&path).unwrap(), body);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn it_200_restart_when_range_ignored() {
        let body = test_body(1024 * 1024);
        let total = body.len() as u64;
        let body_srv = body.clone();
        let url = serve(3, move |i, req| match i {
            0 if req.method == "HEAD" => meta_resp(total, false), // 不宣告 Accept-Ranges
            _ => FakeResp::ok(
                vec![("Content-Length", total.to_string())],
                body_srv.clone(),
            ),
        });
        let path = tmp_file("range-ignored");
        fs::write(&path, &body[..total as usize / 2]).unwrap(); // 半个旧文件
        let (ok, reason, events) = run_to_end(TaskSpec {
            url,
            alt_url: None,
            save_path: path.clone(),
            proxy: None,
            segments: 8,
            timeout_seconds: 20,
        });
        assert!(ok, "下载失败: {reason}");
        assert!(
            events.iter().any(|e| matches!(
                e,
                UiEvent::Log { level: LogLevel::Warn, msg } if msg.contains("不支持断点续传")
            )),
            "应有「不支持断点续传」日志"
        );
        assert_eq!(fs::read(&path).unwrap(), body);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn it_segmented_download_and_assemble() {
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
        });
        let path = tmp_file("segmented");
        let (ok, reason, _) = run_to_end(TaskSpec {
            url,
            alt_url: None,
            save_path: path.clone(),
            proxy: None,
            segments: 8,
            timeout_seconds: 20,
        });
        assert!(ok, "分段下载失败: {reason}");
        assert_eq!(fs::read(&path).unwrap(), body, "合并结果应与源一致");
        assert!(!part_path(&path, 0).exists(), "part 文件应被清理");
        assert!(!sidecar_path(&path).exists(), "sidecar 应被清理");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn it_segmented_resume_after_cancel() {
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
        });
        let path = tmp_file("segmented-resume");
        {
            let (tx, rx) = std::sync::mpsc::channel();
            let ctrl = TaskControl::new();
            let _handle = spawn_task(
                1,
                TaskSpec {
                    url: url.clone(),
                    alt_url: None,
                    save_path: path.clone(),
                    proxy: None,
                    segments: 8,
                    timeout_seconds: 20,
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
                if let Ok(UiEvent::TaskFinished { .. }) = rx.try_recv() {
                    panic!("阶段 A 不应完成");
                }
                thread::sleep(Duration::from_millis(10));
            }
            loop {
                match rx.recv_timeout(Duration::from_secs(10)) {
                    Ok(UiEvent::TaskFinished { .. }) => break,
                    Ok(_) => {}
                    Err(_) => panic!("阶段 A 未收到结束事件"),
                }
            }
        }
        let parts_left: usize = (0..8).filter(|&i| part_len(&path, i) > 0).count();
        assert!(parts_left > 0, "取消后应保留分段文件");
        assert!(sidecar_path(&path).exists(), "取消后应保留 sidecar");

        // 阶段 B：同一 URL 续传完成
        let (ok, reason, events) = run_to_end(TaskSpec {
            url,
            alt_url: None,
            save_path: path.clone(),
            proxy: None,
            segments: 8,
            timeout_seconds: 20,
        });
        assert!(ok, "续传下载失败: {reason}");
        assert_eq!(fs::read(&path).unwrap(), body, "续传合并结果应与源一致");
        assert!(
            events.iter().any(|e| matches!(
                e,
                UiEvent::Log { level: LogLevel::Info, msg } if msg.contains("段已完成")
            )),
            "应识别到已完成分段并续传，事件: {events:?}"
        );
        assert!(!part_path(&path, 0).exists());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn it_alt_url_fallback_on_404() {
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
        });
        let path = tmp_file("alt-404");
        let (ok, reason, _) = run_to_end(TaskSpec {
            url: format!("{base}/main.zip"),
            alt_url: Some(format!("{base}/master.zip")),
            save_path: path.clone(),
            proxy: None,
            segments: 8,
            timeout_seconds: 20,
        });
        assert!(ok, "备用地址兜底失败: {reason}");
        assert_eq!(fs::read(&path).unwrap(), body);
        let _ = fs::remove_file(&path);
    }
}
