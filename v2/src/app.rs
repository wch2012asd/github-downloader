//! 应用主体：状态、后台线程事件路由与全部界面。
//!
//! 布局：自绘标题栏（仅窗口基础功能）/ 双栏内容区（无滚动，撑满）/ 底部状态栏。

use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use chrono::Local;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Margin, Pos2, RichText, Sense, Stroke, StrokeKind,
    TextEdit, UiBuilder, Vec2,
};

use crate::core::config::{self, Config, HistoryEntry};
use crate::core::downloader::{self, TaskControl, TaskSpec};
use crate::core::engines::{self, CHANNELS};
use crate::core::fmt;
use crate::core::proxy;
use crate::core::url_parser::{parse_github_url, LinkType, ParsedUrl};
use crate::events::{LogLevel, UiEvent};

use crate::ui::icons::Icon;
use crate::ui::theme::{self, Theme};
use crate::ui::widgets::{self, Btn};

const PROXY_CHECK_INTERVAL: Duration = Duration::from_secs(30);
const URL_PARSE_DEBOUNCE: Duration = Duration::from_millis(500);
const LOG_MAX: usize = 800;
const LOG_TRIM_TO: usize = 600;
const TOAST_TTL: Duration = Duration::from_secs(3);
const SPEED_HISTORY_CAP: usize = 24;
const TITLEBAR_H: f32 = 40.0;
const FOOTER_H: f32 = 28.0;
const INPUT_H: f32 = 34.0;
const ACTION_BTN: Vec2 = Vec2::new(92.0, 32.0);

const QUICK_PORTS: [u16; 3] = [7890, 7891, 10808];
const LOG_LEVELS: [&str; 6] = ["全部", "INFO", "SUCCESS", "WARN", "ERROR", "PROXY"];

/// 任务的最终结论（用于状态徽章与按钮组形态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Idle,
    Completed,
    Failed,
}

pub struct App {
    events_tx: Sender<UiEvent>,
    events_rx: Receiver<UiEvent>,
    current_port: Arc<AtomicU16>,
    /// 剪贴板监测开关的跨线程视图
    auto_clipboard_flag: Arc<AtomicBool>,
    config: Config,
    th: Theme,
    /// 标题栏展示的应用图标（来自内嵌 app_icon.ico）
    app_icon: Option<egui::TextureHandle>,

    // ── 链接区 ──
    url_input: String,
    parsed: Option<ParsedUrl>,
    estimated_size: Option<u64>,
    size_gen: u64,
    parse_at: Option<Instant>,

    // ── 保存区 ──
    filename: String,
    auto_open: bool,
    disk_free: String,
    last_free_dir: String,

    // ── 网络区 ──
    port_input: String,
    proxy_online: bool,
    proxy_checked_once: bool,
    proxy_port: Option<u16>,
    proxy_latency: Option<u32>,
    /// 每通道测速结果：None=未测速，Some(None)=不可用
    channel_latency: [Option<Option<u32>>; 5],

    // ── 进度区 ──
    running: bool,
    paused: bool,
    outcome: Outcome,
    task_gen: u64,
    task_ctrl: Option<TaskControl>,
    last_downloaded: u64,
    progress_frac: f32,
    speed_history: VecDeque<f64>,

    // ── 日志与弹窗 ──
    logs: VecDeque<(String, LogLevel, String)>,
    log_search: String,
    log_filter: String,
    autoscroll: bool,
    show_history: bool,
    show_settings: bool,

    // ── 窗口 ──
    maximized: bool,
    toast: Option<(String, Instant)>,
    /// 标题栏主题按钮的左边界（拖拽区的右边界），由每帧标题栏绘制时更新
    title_drag_right: Option<f32>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        load_system_fonts(&cc.egui_ctx);

        let config = config::load();
        let port = config.proxy_port;
        let (tx, rx) = std::sync::mpsc::channel();
        let th = if config.theme_dark {
            theme::dark()
        } else {
            theme::light()
        };
        apply_theme(&cc.egui_ctx, &th);
        let app_icon = load_app_icon(&cc.egui_ctx);

        let auto_clipboard_flag = Arc::new(AtomicBool::new(config.auto_clipboard));
        let mut app = Self {
            events_tx: tx.clone(),
            events_rx: rx,
            current_port: Arc::new(AtomicU16::new(port)),
            auto_clipboard_flag,
            config,
            th,
            app_icon,
            url_input: String::new(),
            parsed: None,
            estimated_size: None,
            size_gen: 0,
            parse_at: None,
            filename: String::new(),
            auto_open: false,
            disk_free: "可用: --".into(),
            last_free_dir: String::new(),
            port_input: port.to_string(),
            proxy_online: false,
            proxy_checked_once: false,
            proxy_port: None,
            proxy_latency: None,
            channel_latency: [None; 5],
            running: false,
            paused: false,
            outcome: Outcome::Idle,
            task_gen: 0,
            task_ctrl: None,
            last_downloaded: 0,
            progress_frac: 0.0,
            speed_history: VecDeque::new(),
            logs: VecDeque::new(),
            log_search: String::new(),
            log_filter: "全部".into(),
            autoscroll: true,
            show_history: false,
            show_settings: false,
            maximized: false,
            toast: None,
            title_drag_right: None,
        };
        app.finalize_new(tx);
        app
    }

    fn finalize_new(&mut self, tx: Sender<UiEvent>) {
        self.auto_open = self.config.auto_open;
        self.refresh_disk_free();
        self.push_log(LogLevel::Info, "欢迎使用 GitHub 加速下载器，粘贴链接即可开始");

        // 代理巡检线程
        let tx_proxy = tx.clone();
        let current_port = self.current_port.clone();
        thread::spawn(move || loop {
            let port = current_port.load(Ordering::Relaxed);
            let mut result: Option<(Option<u16>, Option<u32>)> = None;
            if port != 0 && proxy::port_open(port, "127.0.0.1") {
                let latency = proxy::ping_proxy(port, "127.0.0.1");
                result = Some((Some(port), latency));
            }
            if result.map(|(_, l)| l.is_none()).unwrap_or(true) {
                result = Some(proxy::detect_clash().map_or((None, None), |(p, l)| (Some(p), l)));
            }
            if let Some((p, l)) = result {
                let _ = tx_proxy.send(UiEvent::ProxyCheck { port: p, latency: l });
            }
            thread::sleep(PROXY_CHECK_INTERVAL);
        });

        // 通道测速线程
        let tx_lat = tx.clone();
        let port = self.config.proxy_port;
        let prefix = self.config.ghproxy_prefix.clone();
        let kghost = self.config.kkgithub_host.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(1000));
            for ch in CHANNELS.iter() {
                let latency = engines::measure_latency(ch.id, Some(port), &prefix, &kghost);
                let _ = tx_lat.send(UiEvent::ChannelLatency { id: ch.id, latency });
            }
        });

        // 剪贴板监测线程
        let tx_clip = tx.clone();
        let flag = self.auto_clipboard_flag.clone();
        thread::spawn(move || {
            let mut last = String::new();
            loop {
                thread::sleep(Duration::from_millis(1000));
                if !flag.load(Ordering::Relaxed) {
                    continue;
                }
                let Ok(text) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) else {
                    continue;
                };
                let trimmed = text.trim().to_string();
                if trimmed == last || trimmed.is_empty() {
                    continue;
                }
                last = trimmed.clone();
                if parse_github_url(&trimmed).link_type != LinkType::Unknown {
                    let _ = tx_clip.send(UiEvent::ClipboardLink { text: trimmed });
                }
            }
        });
    }

    // ════════════════ 事件路由 ════════════════

    fn handle_events(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.events_rx.try_recv() {
            match ev {
                UiEvent::Log { level, msg } => self.push_log(level, msg),
                UiEvent::Progress { task_gen, progress } if task_gen == self.task_gen => {
                    self.last_downloaded = progress.downloaded;
                    self.progress_frac = (progress.pct / 100.0).clamp(0.0, 1.0) as f32;
                    self.speed_history.push_back(progress.speed_bps);
                    while self.speed_history.len() > SPEED_HISTORY_CAP {
                        self.speed_history.pop_front();
                    }
                }
                UiEvent::TaskFinished { task_gen, ok, reason } if task_gen == self.task_gen => {
                    self.running = false;
                    self.paused = false;
                    self.task_ctrl = None;
                    let name = self.filename.trim().to_string();
                    let url = self.url_input.trim().to_string();
                    self.record_history(name, self.last_downloaded, url, ok);
                    if ok {
                        self.outcome = Outcome::Completed;
                        self.push_log(LogLevel::Success, "任务完成");
                        self.show_toast("下载完成！");
                        if self.config.sound_enabled {
                            play_chime();
                        }
                        if self.auto_open {
                            let dir = self.config.save_dir.clone();
                            self.open_dir(&dir);
                        }
                    } else {
                        self.outcome = Outcome::Failed;
                        if !reason.is_empty() && reason != "已取消" {
                            self.push_log(LogLevel::Error, format!("下载失败: {reason}"));
                        } else {
                            self.push_log(LogLevel::Warn, "下载任务已取消，临时缓存已清除");
                        }
                    }
                }
                UiEvent::SizeFetched { task_gen, size } if task_gen == self.size_gen => {
                    if let Some(s) = size {
                        self.estimated_size = Some(s);
                    }
                }
                UiEvent::ProxyCheck { port, latency } => self.on_proxy_checked(port, latency),
                UiEvent::ProxyTest { port, latency } => match latency {
                    Some(l) => {
                        self.on_proxy_checked(Some(port), Some(l));
                        self.push_log(
                            LogLevel::Success,
                            format!("端口 {port} 握手成功，GitHub API 连通性测试通过 ({l}ms)"),
                        );
                        self.show_toast(&format!("测试连接成功: {l}ms"));
                    }
                    None => self.push_log(
                        LogLevel::Error,
                        format!("代理连接失败 (端口 {port})，请检查 Clash 核心是否运行"),
                    ),
                },
                UiEvent::ChannelLatency { id, latency } => {
                    if let Some(i) = CHANNELS.iter().position(|c| c.id == id) {
                        self.channel_latency[i] = Some(latency);
                    }
                    if id == "clash" && latency.is_some() && self.proxy_latency.is_none() {
                        self.proxy_latency = latency;
                    }
                }
                UiEvent::ClipboardLink { text } => {
                    self.url_input = text.clone();
                    self.parse_url(false);
                    self.push_log(LogLevel::Info, format!("剪贴板监测到链接，已自动填入: {text}"));
                    self.show_toast("已从剪贴板自动填入链接");
                    ctx.request_repaint();
                }
                _ => {} // 过期代数的事件
            }
        }
    }

    fn on_proxy_checked(&mut self, port: Option<u16>, latency: Option<u32>) {
        if let Some(port) = port {
            if self.port_input.trim() != port.to_string() {
                self.port_input = port.to_string();
            }
            if self.config.proxy_port != port {
                self.config.proxy_port = port;
                self.save_config();
            }
            self.proxy_port = Some(port);
            self.proxy_latency = latency;
            self.current_port.store(port, Ordering::Relaxed);
            // Clash 通道卡片与代理巡检同源，巡检成功即更新其延迟显示
            if let Some(i) = CHANNELS.iter().position(|c| c.id == "clash") {
                self.channel_latency[i] = Some(latency);
            }
            if !self.proxy_online {
                let lat = latency.map(|l| format!(" (延迟: {l}ms)")).unwrap_or_default();
                self.push_log(
                    LogLevel::Success,
                    format!("检测到 Clash 代理已启动于 127.0.0.1:{port}{lat}，网络就绪"),
                );
            }
            self.proxy_online = true;
        } else {
            self.proxy_port = None;
            self.proxy_latency = None;
            self.current_port.store(0, Ordering::Relaxed);
            if self.proxy_online {
                self.push_log(LogLevel::Warn, "代理连接已断开，请检查 Clash 是否还在运行");
            } else if !self.proxy_checked_once {
                self.push_log(LogLevel::Warn, "未检测到 Clash 代理，将自动重试");
            }
            self.proxy_online = false;
        }
        self.proxy_checked_once = true;
    }

    // ════════════════ 链接与解析 ════════════════

    fn schedule_parse(&mut self) {
        self.parse_at = Some(Instant::now() + URL_PARSE_DEBOUNCE);
    }

    fn maybe_parse_debounced(&mut self) {
        if let Some(at) = self.parse_at {
            if Instant::now() >= at {
                self.parse_at = None;
                self.parse_url(false);
            }
        }
    }

    fn parse_url(&mut self, announce_empty: bool) {
        let url = self.url_input.trim().to_string();
        if url.is_empty() {
            if announce_empty {
                self.push_log(LogLevel::Warn, "解析失败: 请先输入有效的 GitHub 链接");
                self.show_toast("请先输入 GitHub 链接");
            }
            return;
        }
        let parsed = parse_github_url(&url);
        if parsed.link_type == LinkType::Unknown {
            self.push_log(
                LogLevel::Error,
                "未能识别该链接的 GitHub 资源结构，请检查链接完整性",
            );
            self.parsed = None;
            self.show_toast("链接格式未识别");
            return;
        }
        self.filename = parsed.suggested_filename.clone();
        self.push_log(
            LogLevel::Success,
            format!(
                "解析成功 [{}] 文件名: {}",
                parsed.type_label(),
                parsed.suggested_filename
            ),
        );
        self.parsed = Some(parsed);
        self.fetch_size();
    }

    fn paste_url(&mut self) {
        match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Ok(text) => {
                let trimmed = text.trim().to_string();
                if trimmed.is_empty() {
                    self.show_toast("剪贴板为空");
                    return;
                }
                self.url_input = trimmed;
                self.push_log(LogLevel::Info, "已从剪贴板读取内容");
                self.parse_url(false);
            }
            Err(_) => self.show_toast("无法访问剪贴板，请使用 Ctrl+V"),
        }
    }

    fn fetch_size(&mut self) {
        let Some(parsed) = self.parsed.clone() else {
            return;
        };
        self.size_gen += 1;
        let task_gen = self.size_gen;
        let engine = self.config.engine.clone();
        let url = engines::build_download_url(
            &engine,
            &parsed.download_url,
            &self.config.ghproxy_prefix,
            &self.config.kkgithub_host,
        );
        let alt = self.alt_url_for(&parsed, &engine);
        let proxy = engines::proxies_for(&engine, self.proxy_port);
        let timeout = self.config.timeout_seconds;
        let tx = self.events_tx.clone();
        thread::spawn(move || {
            let mut size = downloader::fetch_size(&url, proxy.as_deref(), timeout);
            if size.is_none() {
                if let Some(a) = &alt {
                    size = downloader::fetch_size(a, proxy.as_deref(), timeout);
                }
            }
            let _ = tx.send(UiEvent::SizeFetched { task_gen: task_gen, size });
        });
    }

    fn alt_url_for(&self, parsed: &ParsedUrl, engine: &str) -> Option<String> {
        if parsed.link_type != LinkType::RepoPage {
            return None;
        }
        let master = format!(
            "https://github.com/{}/{}/archive/refs/heads/master.zip",
            parsed.owner, parsed.repo
        );
        Some(engines::build_download_url(
            engine,
            &master,
            &self.config.ghproxy_prefix,
            &self.config.kkgithub_host,
        ))
    }

    fn select_engine(&mut self, id: &'static str) {
        if self.config.engine != id {
            self.config.engine = id.into();
            self.save_config();
            self.push_log(
                LogLevel::Proxy,
                format!("切换加速模式为: {}", engines::channel(id).name),
            );
            self.fetch_size();
        }
    }

    // ════════════════ 下载控制 ════════════════

    fn on_primary_click(&mut self) {
        if self.running {
            let Some(ctrl) = self.task_ctrl.clone() else {
                return;
            };
            if self.paused {
                ctrl.resume();
                self.paused = false;
                self.push_log(LogLevel::Info, "正在断点续传下载...");
            } else {
                ctrl.pause();
                self.paused = true;
                self.push_log(LogLevel::Warn, "下载任务已暂停");
            }
        } else {
            self.start_download();
        }
    }

    fn cancel_download(&mut self) {
        if let Some(ctrl) = &self.task_ctrl {
            ctrl.stop();
        }
    }

    fn start_download(&mut self) {
        let url_raw = self.url_input.trim().to_string();
        if url_raw.is_empty() {
            self.push_log(LogLevel::Warn, "下载未启动: 请先提供下载链接");
            self.show_toast("请先输入 GitHub 链接");
            return;
        }
        let parsed = parse_github_url(&url_raw);
        if parsed.link_type == LinkType::Unknown {
            self.push_log(LogLevel::Warn, "无法识别此 GitHub 链接类型");
            return;
        }

        let generic = parsed.link_type == LinkType::GeneralUrl;
        let mut engine = self.config.engine.clone();

        // 外部直链：ghproxy/kkgithub/moeyy 仅加速 GitHub，自动降级为本地代理或直连
        if generic && matches!(engine.as_str(), "ghproxy" | "kkgithub" | "moeyy") {
            let fallback = if self.proxy_port.is_some() { "clash" } else { "direct" };
            self.push_log(
                LogLevel::Proxy,
                format!("镜像通道仅适用于 GitHub 链接，已改用 {}", engines::channel(fallback).name),
            );
            engine = fallback.into();
            self.config.engine = engine.clone();
        }
        // GitHub 链接：Clash 未在线时的既有镜像回退
        if !generic && engine == "clash" && self.proxy_port.is_none() {
            self.push_log(LogLevel::Warn, "未检测到 Clash 代理，已自动切换备用 GHProxy 镜像通道");
            engine = "ghproxy".into();
            self.config.engine = engine.clone();
        }
        let proxy = engines::proxies_for(&engine, self.proxy_port);

        let raw_name = if self.filename.trim().is_empty() {
            parsed.suggested_filename.clone()
        } else {
            self.filename.trim().to_string()
        };
        let filename = sanitize_filename(&raw_name);
        self.filename = filename.clone();

        let save_dir = self.config.save_dir.clone();
        let save_path = Path::new(&save_dir).join(&filename);
        let final_url = engines::build_download_url(
            &engine,
            &parsed.download_url,
            &self.config.ghproxy_prefix,
            &self.config.kkgithub_host,
        );
        let alt_url = self.alt_url_for(&parsed, &engine);

        if let Ok(p) = self.port_input.trim().parse::<u16>() {
            self.config.proxy_port = p;
        }
        self.save_config();

        self.push_log(LogLevel::Info, format!("准备下载任务: {filename}"));
        self.push_log(LogLevel::Info, format!("目标路径: {}", save_path.display()));
        self.push_log(
            LogLevel::Proxy,
            format!(
                "启用引擎: {} · 并发线程数: {}",
                engine.to_uppercase(),
                self.config.segments
            ),
        );

        self.task_gen += 1;
        let ctrl = TaskControl::new();
        self.task_ctrl = Some(ctrl.clone());
        downloader::spawn_task(
            self.task_gen,
            TaskSpec {
                url: final_url,
                alt_url,
                save_path,
                proxy,
                segments: self.config.segments,
                timeout_seconds: self.config.timeout_seconds,
            },
            ctrl,
            self.events_tx.clone(),
        );

        self.running = true;
        self.paused = false;
        self.outcome = Outcome::Idle;
        self.progress_frac = 0.0;
        self.speed_history.clear();
    }

    fn download_directly(&mut self) {
        let parsed = parse_github_url(self.url_input.trim());
        if parsed.link_type == LinkType::Unknown {
            self.show_toast("请先输入有效的 GitHub 链接");
            return;
        }
        let url = engines::build_download_url(
            &self.config.engine,
            &parsed.download_url,
            &self.config.ghproxy_prefix,
            &self.config.kkgithub_host,
        );
        self.push_log(
            LogLevel::Proxy,
            format!(
                "目标下载地址 ({}): {}",
                self.config.engine.to_uppercase(),
                url
            ),
        );
        // 交给系统默认浏览器接管下载
        let ok = std::process::Command::new("cmd")
            .args(["/c", "start", "", &url])
            .spawn()
            .is_ok();
        if ok {
            self.push_log(LogLevel::Success, "浏览器已接管下载请求");
            self.show_toast("已触发浏览器直链下载");
        } else {
            self.push_log(LogLevel::Error, "无法调用系统浏览器");
        }
    }

    fn record_history(&mut self, name: String, size: u64, url: String, ok: bool) {
        let entry = HistoryEntry {
            time: Local::now().format("%Y-%m-%d %H:%M").to_string(),
            name,
            size,
            url,
            ok,
            save_path: self.config.save_dir.clone(),
        };
        config::push_history(&mut self.config.history, entry);
        self.save_config();
    }

    fn open_dir(&mut self, dir: &str) {
        if !Path::new(dir).is_dir() {
            self.push_log(LogLevel::Warn, format!("目录不存在: {dir}"));
            return;
        }
        let _ = std::process::Command::new("explorer").arg(dir).spawn();
    }

    fn show_toast(&mut self, msg: &str) {
        self.toast = Some((msg.to_string(), Instant::now()));
    }

    // ════════════════ 保存目录 ════════════════

    fn set_save_dir(&mut self, path: String) {
        self.config.save_dir = path;
        self.refresh_disk_free();
    }

    fn choose_dir(&mut self) {
        if let Some(dir) = rfd::FileDialog::new()
            .set_directory(&self.config.save_dir)
            .pick_folder()
        {
            self.set_save_dir(dir.to_string_lossy().into_owned());
        }
    }

    fn quick_dirs(&self) -> Vec<(&'static str, String)> {
        let exe_dir = config::config_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let home = std::env::var("USERPROFILE").unwrap_or_default();
        vec![
            ("默认目录", exe_dir),
            ("下载目录", format!("{home}\\Downloads")),
            ("桌面", format!("{home}\\Desktop")),
        ]
    }

    fn refresh_disk_free(&mut self) {
        if self.last_free_dir != self.config.save_dir {
            self.last_free_dir = self.config.save_dir.clone();
            self.disk_free = disk_free_bytes(Path::new(&self.config.save_dir))
                .map(|free| format!("可用: {}", fmt::format_size(free)))
                .unwrap_or_else(|| "可用: --".into());
        }
    }

    fn save_config(&self) {
        if !config::save(&self.config) {
            let _ = self.events_tx.send(UiEvent::Log {
                level: LogLevel::Warn,
                msg: "配置保存失败，设置不会被记住".into(),
            });
        }
    }

    // ════════════════ 日志 ════════════════

    fn push_log(&mut self, level: LogLevel, msg: impl Into<String>) {
        let ts = Local::now().format("[%H:%M:%S]").to_string();
        self.logs.push_back((ts, level, msg.into()));
        if self.logs.len() > LOG_MAX {
            let drop = self.logs.len() - LOG_TRIM_TO;
            self.logs.drain(..drop);
        }
    }

    fn log_passes(&self, level: LogLevel, msg: &str) -> bool {
        if self.log_filter != "全部" && level.as_str() != self.log_filter {
            return false;
        }
        let kw = self.log_search.trim().to_lowercase();
        kw.is_empty() || msg.to_lowercase().contains(&kw)
    }

    // ════════════════ 每帧绘制 ════════════════

    fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.handle_events(&ctx);
        self.maybe_parse_debounced();
        self.refresh_disk_free();
        if let Some((_, t)) = &self.toast {
            if t.elapsed() > TOAST_TTL {
                self.toast = None;
            }
        }

        let th = self.th;
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(th.window_bg).inner_margin(Margin::ZERO))
            .show(ui, |ui| {
                // 三个互不重叠的显式区域：标题栏 / 内容 / 状态栏
                self.title_bar(ui, &ctx);

                let avail = ui.available_rect_before_wrap();
                let content = egui::Rect::from_min_max(
                    avail.left_top(),
                    Pos2::new(avail.right(), avail.bottom() - FOOTER_H),
                );
                let inner = content.shrink(12.0);
                let col_w = (inner.width() - 12.0) / 2.0;
                let left_rect = egui::Rect::from_min_max(
                    inner.left_top(),
                    Pos2::new(inner.left() + col_w, inner.bottom()),
                );
                let right_rect = egui::Rect::from_min_max(
                    Pos2::new(inner.right() - col_w, inner.top()),
                    inner.right_bottom(),
                );

                // 列内禁用隐式垂直间距：卡片间隙与撑满高度全部显式控制，
                // 否则隐式间距会累计使“撑满”计算溢出（日志卡被状态栏遮挡的根因）
                let mut left = ui.new_child(UiBuilder::new().max_rect(left_rect));
                left.spacing_mut().item_spacing.y = 0.0;
                self.link_card(&mut left);
                left.add_space(6.0);
                self.save_card(&mut left);
                left.add_space(6.0);
                self.network_card(&mut left);

                let mut right = ui.new_child(UiBuilder::new().max_rect(right_rect));
                right.spacing_mut().item_spacing.y = 0.0;
                self.progress_card(&mut right);
                right.add_space(6.0);
                // 日志卡限定在剩余精确矩形内：高度由容器决定，内部自行撑满
                let log_rect = egui::Rect::from_min_max(
                    right.cursor().left_top(),
                    right.max_rect().right_bottom(),
                );
                right.add_space(log_rect.height());
                let mut log_child = right.new_child(UiBuilder::new().max_rect(log_rect));
                log_child.spacing_mut().item_spacing.y = 0.0;
                self.log_card(&mut log_child);

                self.footer(ui);
            });

        self.draw_toast(ui);
        if self.show_history {
            self.history_window(ui);
        }
        if self.show_settings {
            self.settings_window(ui);
        }
        ui.ctx().request_repaint_after(Duration::from_millis(150));
    }

    // ── 自定义标题栏：仅品牌与窗口控制 ──

    fn title_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let th = self.th;
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), TITLEBAR_H), Sense::hover());
        ui.painter().rect_filled(rect, 0, th.titlebar_bg);
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(1.0, th.card_border),
        );

        let mut bar = ui.new_child(UiBuilder::new().max_rect(rect));
        bar.horizontal_centered(|ui| {
            ui.add_space(10.0);
            if let Some(tex) = &self.app_icon {
                ui.image(egui::load::SizedTexture::new(tex, Vec2::splat(22.0)));
            }
            ui.label(
                RichText::new("GitHub 加速下载器")
                    .strong()
                    .size(13.0)
                    .color(th.text),
            );
            widgets::badge(ui, &th, concat!("v", env!("CARGO_PKG_VERSION")));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // right_to_left 布局先添加的靠右：依次添加 关闭/最大化/最小化/分隔/主题
                // 视觉顺序为（左→右）主题 | 最小化 最大化 关闭（Windows 标准）
                if widgets::window_btn(ui, &th, Icon::Close, "关闭", true).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if widgets::window_btn(
                    ui,
                    &th,
                    Icon::Maximize,
                    if self.maximized { "还原" } else { "最大化" },
                    false,
                )
                .clicked()
                {
                    self.maximized = !self.maximized;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.maximized));
                }
                if widgets::window_btn(ui, &th, Icon::Minus, "最小化", false).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
                ui.add_space(6.0);
                ui.label(RichText::new("|").size(11.0).color(th.sep));
                ui.add_space(6.0);
                let theme_icon = if th.dark { Icon::Sun } else { Icon::Moon };
                let theme_btn = widgets::titlebar_icon_btn(
                    ui,
                    &th,
                    theme_icon,
                    if th.dark { "切换为明亮模式" } else { "切换为深色模式" },
                    th.hover,
                );
                self.title_drag_right = Some(theme_btn.rect.left());
                if theme_btn.clicked() {
                    self.config.theme_dark = !self.config.theme_dark;
                    self.th = if self.config.theme_dark {
                        theme::dark()
                    } else {
                        theme::light()
                    };
                    apply_theme(ctx, &self.th);
                    self.save_config();
                }
            });
        });

        // 拖拽区取主题按钮左侧的整段空白（后注册在最上层，与按钮互不抢点击）；
        // 仅在拖拽开始的瞬间移交系统模态循环，反复发送会导致窗口抖动
        let drag_right = self.title_drag_right.unwrap_or(rect.center().x);
        let drag_zone = egui::Rect::from_min_max(
            rect.left_top(),
            Pos2::new(drag_right.max(rect.left()), rect.bottom()),
        );
        let drag = ui.interact(drag_zone, egui::Id::new("title_bar_drag"), Sense::click_and_drag());
        if drag.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if drag.double_clicked() {
            self.maximized = !self.maximized;
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.maximized));
        }
    }

    // ── 底部状态栏 ──

    fn footer(&mut self, ui: &mut egui::Ui) {
        let th = self.th;
        let avail = ui.available_rect_before_wrap();
        let rect = egui::Rect::from_min_max(
            Pos2::new(avail.left(), avail.bottom() - FOOTER_H),
            avail.right_bottom(),
        );
        ui.painter().rect_filled(rect, 0, th.footer_bg);
        ui.painter().line_segment(
            [rect.left_top(), rect.right_top()],
            Stroke::new(1.0, th.card_border),
        );
        let mut foot = ui.new_child(UiBuilder::new().max_rect(rect.shrink2(Vec2::new(16.0, 0.0))));
        foot.horizontal_centered(|ui| {
            let mono =
                |s: String| RichText::new(s).font(FontId::monospace(11.0)).color(th.text_muted);
            let dir = self
                .config
                .save_dir
                .rsplit('\\')
                .next()
                .unwrap_or("dist")
                .to_string();
            ui.label(mono(format!("存储: {dir}")));
            ui.label(RichText::new("|").size(11.0).color(th.sep));
            ui.label(mono(format!("引擎: {}", self.config.engine.to_uppercase())));
            ui.label(RichText::new("|").size(11.0).color(th.sep));
            let port = self.proxy_port.unwrap_or(self.config.proxy_port);
            ui.label(mono(format!("代理: 127.0.0.1:{port}")));
            if self.proxy_online {
                let lat = self.proxy_latency.map(|l| format!(" ({l}ms)")).unwrap_or_default();
                ui.label(
                    RichText::new(format!("· 代理正常{lat}"))
                        .font(FontId::monospace(11.0))
                        .color(th.emerald_text),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(mono(format!("分块传输: {}T", self.config.segments)));
            });
        });
    }

    // ── Toast ──

    fn draw_toast(&mut self, ui: &mut egui::Ui) {
        let Some((msg, _)) = self.toast.clone() else {
            return;
        };
        let th = self.th;
        egui::Area::new(egui::Id::new("toast"))
            .anchor(Align2::CENTER_TOP, [0.0, 48.0])
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::default()
                    .fill(if th.dark {
                        Color32::from_rgba_unmultiplied(245, 245, 245, 242)
                    } else {
                        Color32::from_rgba_unmultiplied(23, 23, 23, 230)
                    })
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(14, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            crate::ui::icons::show(
                                ui,
                                Icon::CheckCircle,
                                13.0,
                                if th.dark { th.accent } else { th.accent_text },
                                th.card_bg,
                            );
                            ui.label(
                                RichText::new(&msg).size(13.0).color(if th.dark {
                                    Color32::from_rgb(0x17, 0x17, 0x17)
                                } else {
                                    Color32::WHITE
                                }),
                            );
                        });
                    });
            });
    }
}

// ────────────────────────── 五张卡片 ──────────────────────────

impl App {
    fn link_card(&mut self, ui: &mut egui::Ui) {
        let th = self.th;
        widgets::card(ui, &th, |ui| {
            widgets::section_header(
                ui,
                &th,
                Icon::Link,
                th.accent_text,
                "GitHub 链接",
                Some("· 自动识别 Release/源码/Raw"),
                |_ui| {},
            );
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                let w = ui.available_width() - 164.0;
                let resp = ui.add_sized(
                    Vec2::new(w, INPUT_H),
                    TextEdit::singleline(&mut self.url_input)
                        .hint_text("粘贴下载链接（GitHub / 官网直链均可）")
                        .font(egui::TextStyle::Monospace)
                        .vertical_align(egui::Align::Center),
                );
                if resp.changed() {
                    self.schedule_parse();
                }
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.parse_url(true);
                }
                if widgets::button(ui, &th, Some(Icon::Clipboard), "粘贴", &Btn::Neutral, true, Vec2::new(72.0, INPUT_H))
                    .clicked()
                {
                    self.paste_url();
                }
                if widgets::button(ui, &th, Some(Icon::ArrowRight), "解析", &Btn::Primary, true, Vec2::new(72.0, INPUT_H))
                    .clicked()
                {
                    self.parse_url(true);
                }
            });
            ui.add_space(6.0);

            ui.separator();
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("资源类型:").size(12.0).color(th.text_muted));
                let Some(parsed) = self.parsed.clone() else {
                    let hint = if self.url_input.trim().is_empty() {
                        "待输入"
                    } else {
                        "尚未解析（点击「解析」提取）"
                    };
                    ui.label(RichText::new(hint).size(12.0).color(th.text_faint));
                    return;
                };
                let (icon, tint) = match parsed.link_type {
                    LinkType::ReleaseAsset => (Icon::Package, th.accent_text),
                    LinkType::RepoArchive => (Icon::Folder, th.amber),
                    LinkType::RawFile => (Icon::FileCode, th.emerald_text),
                    _ => (Icon::FileText, th.text_muted),
                };
                crate::ui::icons::show(ui, icon, 14.0, tint, th.card_bg);
                ui.label(
                    RichText::new(parsed.type_label())
                        .size(12.0)
                        .strong()
                        .color(th.text_secondary),
                );
                if !parsed.owner.is_empty() {
                    ui.label(RichText::new("·").size(12.0).color(th.sep));
                    ui.label(
                        RichText::new(format!("{}/{}", parsed.owner, parsed.repo))
                            .font(FontId::new(12.0, egui::FontFamily::Monospace))
                            .color(th.text_muted),
                    );
                }
                let ref_name = if !parsed.tag.is_empty() {
                    Some(parsed.tag.clone())
                } else if !parsed.branch.is_empty() {
                    Some(parsed.branch.clone())
                } else {
                    None
                };
                if let Some(r) = ref_name {
                    ui.label(
                        RichText::new(format!("@{r}"))
                            .font(FontId::new(12.0, egui::FontFamily::Monospace))
                            .color(th.accent_text),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(s) = self.estimated_size {
                        ui.label(
                            RichText::new(fmt::format_size(s))
                                .font(FontId::new(12.0, egui::FontFamily::Monospace))
                                .strong()
                                .color(th.text_secondary),
                        );
                        ui.label(RichText::new("预估大小:").size(12.0).color(th.text_muted));
                    }
                });
            });
        });
    }

    fn save_card(&mut self, ui: &mut egui::Ui) {
        let th = self.th;
        widgets::card(ui, &th, |ui| {
            widgets::section_header(ui, &th, Icon::Folder, th.amber, "保存位置与文件名", None, |ui| {
                crate::ui::icons::show(ui, Icon::HardDrive, 13.0, th.text_faint, th.card_bg);
                ui.label(RichText::new(&self.disk_free).size(12.0).color(th.text_muted));
            });
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.add_space(2.0);
                    ui.label(RichText::new("存储目录:").size(12.0).color(th.text_secondary));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for (label, path) in self.quick_dirs().into_iter().rev() {
                        if widgets::chip(ui, &th, label, self.config.save_dir == path).clicked() {
                            if Path::new(&path).is_dir() {
                                self.set_save_dir(path);
                            } else {
                                self.push_log(LogLevel::Warn, format!("目录不存在: {path}"));
                            }
                        }
                    }
                    ui.label(RichText::new("快捷:").size(11.0).color(th.text_faint));
                });
            });
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                if widgets::button(ui, &th, Some(Icon::Folder), "选择", &Btn::Neutral, true, Vec2::new(68.0, INPUT_H))
                    .clicked()
                {
                    self.choose_dir();
                }
                let w = ui.available_width();
                if ui
                    .add_sized(
                        Vec2::new(w, INPUT_H),
                        TextEdit::singleline(&mut self.config.save_dir)
                            .hint_text("请输入或选择下载存储目录路径")
                            .font(egui::TextStyle::Monospace)
                            .vertical_align(egui::Align::Center),
                    )
                    .changed()
                {
                    self.refresh_disk_free();
                }
            });
            ui.add_space(6.0);

            // 文件名 + 扩展名徽标
            let ext = self
                .filename
                .trim()
                .rsplit('.')
                .next()
                .filter(|e| *e != self.filename.trim())
                .map(|e| e.to_uppercase())
                .unwrap_or_default();
            let w = ui.available_width();
            let resp = ui.add_sized(
                Vec2::new(w, INPUT_H),
                TextEdit::singleline(&mut self.filename)
                    .hint_text("文件保存名称")
                    .font(egui::TextStyle::Monospace)
                    .vertical_align(egui::Align::Center),
            );
            if resp.changed() {
                self.refresh_disk_free();
            }
            if !ext.is_empty() {
                let g = ui.painter().layout(
                    format!(".{ext}"),
                    FontId::new(10.0, egui::FontFamily::Monospace),
                    th.badge_text,
                    f32::INFINITY,
                );
                let br = egui::Rect::from_min_size(
                    resp.rect.right_top()
                        + Vec2::new(-(g.size().x + 18.0), (INPUT_H - g.size().y - 6.0) / 2.0),
                    g.size() + Vec2::new(8.0, 6.0),
                );
                ui.painter().rect_filled(br, CornerRadius::same(4), th.badge_bg);
                ui.painter().galley(br.left_top() + Vec2::new(4.0, 3.0), g, th.badge_text);
            }
        });
    }

    fn network_card(&mut self, ui: &mut egui::Ui) {
        let th = self.th;
        // 与日志卡同一自绘方案：底板从当前光标（前两张卡之后）填到列底-10，
        // 只占剩余空间，不遮挡先绘制的链接/保存卡
        let full = egui::Rect::from_min_max(
            ui.cursor().left_top(),
            Pos2::new(ui.max_rect().right(), ui.max_rect().bottom() - 10.0),
        );
        ui.painter().rect_filled(full, CornerRadius::same(12), th.card_bg);
        ui.painter().rect_stroke(
            full,
            CornerRadius::same(12),
            Stroke::new(1.0, th.card_border),
            StrokeKind::Middle,
        );

        let mut c = ui.new_child(UiBuilder::new().max_rect(full.shrink(14.0)));
        c.spacing_mut().item_spacing.y = 0.0;

        widgets::section_header(&mut c, &th, Icon::Network, th.indigo_text, "网络加速与 Clash 代理", None, |ui| {
            if self.proxy_online {
                crate::ui::icons::show(ui, Icon::CheckCircle, 13.0, th.emerald, th.card_bg);
                let lat = self.proxy_latency.map(|l| format!(" ({l}ms)")).unwrap_or_default();
                ui.label(
                    RichText::new(format!("代理正常{lat}"))
                        .font(FontId::new(12.0, egui::FontFamily::Monospace))
                        .strong()
                        .color(th.emerald_text),
                );
            } else if self.proxy_checked_once {
                crate::ui::icons::show(ui, Icon::XCircle, 13.0, th.rose, th.card_bg);
                ui.label(RichText::new("未检测到代理").size(12.0).color(th.rose));
            } else {
                ui.label(RichText::new("检测中...").size(12.0).color(th.text_faint));
            }
        });
        c.add_space(6.0);

        // 端口配置内嵌面板
        egui::Frame::default()
            .fill(th.inner_panel_bg)
            .stroke(Stroke::new(1.0, th.inner_panel_border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(10))
            .show(&mut c, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("端口:").size(13.0).color(th.text_secondary));
                    if ui
                        .add_sized(
                            Vec2::new(70.0, 30.0),
                            TextEdit::singleline(&mut self.port_input)
                                .font(egui::TextStyle::Monospace)
                                .vertical_align(egui::Align::Center),
                        )
                        .lost_focus()
                    {
                        if let Ok(p) = self.port_input.trim().parse::<u16>() {
                            self.current_port.store(p, Ordering::Relaxed);
                            if self.config.proxy_port != p {
                                self.config.proxy_port = p;
                                self.save_config();
                            }
                        }
                    }
                    ui.add_space(2.0);
                    if widgets::button(ui, &th, Some(Icon::Refresh), "自动检测", &Btn::Neutral, true, Vec2::new(96.0, 30.0))
                        .clicked()
                    {
                        self.push_log(
                            LogLevel::Info,
                            "正在扫描本地 Clash / V2Ray 代理服务端口 (7890, 7891, 10808, 10809)...",
                        );
                        let port = self.port_input.trim().parse::<u16>().unwrap_or(0);
                        self.current_port.store(port, Ordering::Relaxed);
                        let tx = self.events_tx.clone();
                        thread::spawn(move || {
                            let mut result = None;
                            if port != 0 && proxy::port_open(port, "127.0.0.1") {
                                let latency = proxy::ping_proxy(port, "127.0.0.1");
                                result = Some((Some(port), latency));
                            }
                            if result.map(|(_, l)| l.is_none()).unwrap_or(true) {
                                result = Some(
                                    proxy::detect_clash().map_or((None, None), |(p, l)| (Some(p), l)),
                                );
                            }
                            if let Some((p, l)) = result {
                                let _ = tx.send(UiEvent::ProxyCheck { port: p, latency: l });
                            }
                        });
                    }
                    if widgets::button(ui, &th, Some(Icon::Activity), "测试连接", &Btn::Neutral, true, Vec2::new(96.0, 30.0))
                        .clicked()
                    {
                        let Ok(port) = self.port_input.trim().parse::<u16>() else {
                            self.push_log(LogLevel::Warn, "请输入有效的端口号");
                            return;
                        };
                        self.push_log(LogLevel::Info, format!("正在测试端口 {port} 的代理连接..."));
                        let tx = self.events_tx.clone();
                        thread::spawn(move || {
                            let latency = proxy::ping_proxy(port, "127.0.0.1");
                            let _ = tx.send(UiEvent::ProxyTest { port, latency });
                        });
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        for p in QUICK_PORTS.into_iter().rev() {
                            if widgets::chip(ui, &th, &p.to_string(), self.config.proxy_port == p).clicked() {
                                self.port_input = p.to_string();
                                self.current_port.store(p, Ordering::Relaxed);
                            }
                        }
                        ui.label(RichText::new("快捷:").size(11.0).color(th.text_faint));
                    });
                });
            });
        c.add_space(8.0);

        // 通道选择
        c.horizontal(|ui| {
            crate::ui::icons::show(ui, Icon::Zap, 13.0, th.amber, th.card_bg);
            ui.label(RichText::new("加速引擎通道").size(12.0).color(th.text_secondary));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new("点击切换").size(11.0).color(th.text_faint));
            });
        });
        c.add_space(4.0);

        let n = CHANNELS.len();
        let gap = 6.0;
        // 行内统一由 item_spacing 提供间距，避免双倍间距溢出卡片
        c.spacing_mut().item_spacing.x = gap;
        let w = (c.available_width() - gap * (n as f32 - 1.0)) / n as f32;
        c.horizontal(|ui| {
            // 外部直链时镜像通道置灰（仅 GitHub 链接可加速）
            let generic_loaded = self
                .parsed
                .as_ref()
                .is_some_and(|p| p.link_type == LinkType::GeneralUrl);
            for (i, ch) in CHANNELS.iter().enumerate() {
                let selected = self.config.engine == ch.id;
                let dim = generic_loaded && matches!(ch.id, "ghproxy" | "kkgithub" | "moeyy");
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 58.0), Sense::click());
                let p = ui.painter();
                p.rect_filled(rect, CornerRadius::same(8), if selected {
                    th.accent_soft
                } else if ui.rect_contains_pointer(rect) {
                    th.inner_panel_bg
                } else {
                    th.card_bg
                });
                p.rect_stroke(
                    rect,
                    CornerRadius::same(8),
                    Stroke::new(
                        if selected { 1.5 } else { 1.0 },
                        if selected { th.accent } else { th.card_border },
                    ),
                    StrokeKind::Middle,
                );
                let name_color = if dim { th.text_faint } else { th.text };
                p.text(
                    rect.left_top() + Vec2::new(8.0, 8.0),
                    Align2::LEFT_TOP,
                    ch.name,
                    FontId::proportional(12.0),
                    name_color,
                );
                let lat_text = match self.channel_latency[i] {
                    Some(Some(ms)) => format!("{ms}ms"),
                    Some(None) => "不可用".into(),
                    None => ch.latency_hint.into(),
                };
                let lat_color = if dim { th.text_faint } else { th.emerald_text };
                p.text(
                    rect.left_bottom() + Vec2::new(8.0, -8.0),
                    Align2::LEFT_BOTTOM,
                    lat_text,
                    FontId::new(11.0, egui::FontFamily::Monospace),
                    lat_color,
                );
                // 徽标（右下）
                let g = p.layout(
                    ch.tag.to_string(),
                    FontId::new(9.0, egui::FontFamily::Monospace),
                    th.badge_text,
                    f32::INFINITY,
                );
                let badge_size = g.size() + Vec2::new(8.0, 5.0);
                let br = egui::Rect::from_min_size(
                    Pos2::new(rect.right() - badge_size.x - 5.0, rect.bottom() - badge_size.y - 5.0),
                    badge_size,
                );
                p.rect_filled(br, CornerRadius::same(3), th.badge_bg);
                p.galley(br.left_top() + Vec2::new(4.0, 2.5), g, th.badge_text);
                let r = resp.on_hover_text(ch.description);
                if r.clicked() {
                    self.select_engine(ch.id);
                }
            }
        });
    }

    fn progress_card(&mut self, ui: &mut egui::Ui) {
        let th = self.th;
        widgets::card(ui, &th, |ui| {
            widgets::section_header(ui, &th, Icon::Gauge, th.emerald, "下载进度与传输状态", None, |ui| {
                let (label, color, dot) = if self.running && self.paused {
                    ("已暂停", th.amber_text, true)
                } else if self.running {
                    ("下载中...", th.accent_text, true)
                } else {
                    match self.outcome {
                        Outcome::Completed => ("下载完成", th.emerald_text, false),
                        Outcome::Failed => ("下载失败", th.rose, false),
                        Outcome::Idle => ("就绪", th.text_muted, false),
                    }
                };
                if dot {
                    // 下载中呼吸点
                    let pulse = 0.55 + 0.45 * (ui.input(|i| i.time) * 2.0).sin().abs();
                    ui.painter().circle_filled(
                        ui.cursor().left_top() + Vec2::new(4.0, 9.0),
                        3.5,
                        Color32::from_rgba_unmultiplied(
                            color.r(),
                            color.g(),
                            color.b(),
                            (pulse * 255.0) as u8,
                        ),
                    );
                    ui.allocate_exact_size(Vec2::new(11.0, 18.0), Sense::hover());
                }
                ui.label(RichText::new(label).size(13.0).strong().color(color));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let h_btn = widgets::titlebar_icon_btn(ui, &th, Icon::History, "任务记录", th.hover);
                    if h_btn.clicked() {
                        self.show_history = true;
                    }
                    let s_btn = widgets::titlebar_icon_btn(ui, &th, Icon::Settings, "首选项与高级配置", th.hover);
                    if s_btn.clicked() {
                        self.show_settings = true;
                    }
                    ui.add_space(6.0);
                });
            });
            ui.add_space(8.0);

            // 进度条
            let bar_rect = egui::Rect::from_min_size(
                ui.available_rect_before_wrap().left_top(),
                Vec2::new(ui.available_width(), 10.0),
            );
            ui.painter().rect_filled(bar_rect, CornerRadius::same(5), th.inner_panel_bg);
            ui.painter().rect_stroke(
                bar_rect,
                CornerRadius::same(5),
                Stroke::new(1.0, th.card_border),
                StrokeKind::Middle,
            );
            if self.progress_frac > 0.0 {
                let fill_color = if self.outcome == Outcome::Completed {
                    th.emerald
                } else if self.paused {
                    th.amber
                } else {
                    th.accent
                };
                let w = (bar_rect.width() - 2.0) * self.progress_frac;
                if w > 2.0 {
                    let fr = egui::Rect::from_min_size(
                        bar_rect.left_top() + Vec2::new(1.0, 1.0),
                        Vec2::new(w, 8.0),
                    );
                    ui.painter().rect_filled(fr, CornerRadius::same(4), fill_color);
                }
            }
            ui.allocate_exact_size(Vec2::new(0.0, 16.0), Sense::hover());

            // 遥测四格
            let pct = self.progress_frac * 100.0;
            let speed_now = self.speed_history.back().copied().unwrap_or(0.0);
            let total = self.estimated_size;
            let transferred = if total.is_some() || self.last_downloaded > 0 {
                format!(
                    "{} / {}",
                    fmt::format_size(self.last_downloaded),
                    total.map(fmt::format_size).unwrap_or_else(|| "未知".into())
                )
            } else {
                "-- / --".into()
            };
            let eta = if self.running && !self.paused && speed_now > 0.0 && total.is_some() {
                let remain = (total.unwrap() as f64 - self.last_downloaded as f64).max(0.0);
                fmt::format_eta((remain / speed_now) as u64)
            } else {
                "--".into()
            };
            let stats: [(&str, String, Color32, bool); 4] = [
                ("完成度", format!("{pct:.1}%"), th.text, true),
                (
                    "当前速度",
                    if speed_now > 0.0 { fmt::format_speed(speed_now) } else { "--".into() },
                    th.accent_text,
                    true,
                ),
                ("已传输 / 总大小", transferred, th.text_secondary, false),
                (
                    "剩余时间 · 线程",
                    format!("{eta} · {}T", self.config.segments),
                    th.text_secondary,
                    false,
                ),
            ];
            let gap = 8.0;
            ui.spacing_mut().item_spacing.x = gap;
            let w = (ui.available_width() - gap * 3.0) / 4.0;
            ui.horizontal(|ui| {
                for (caption, value, color, bold) in &stats {
                    ui.allocate_ui(Vec2::new(w, 44.0), |ui| {
                        ui.label(RichText::new(*caption).size(11.0).color(th.text_faint));
                        let mut rt = RichText::new(value)
                            .font(FontId::new(14.0, egui::FontFamily::Monospace))
                            .color(*color);
                        if *bold {
                            rt = rt.strong();
                        }
                        ui.label(rt);
                    });
                }
            });

            // 速度曲线（有数据时展示）
            if self.speed_history.len() > 3 {
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("速度波动曲线").size(11.0).color(th.text_faint));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let peak = self.speed_history.iter().cloned().fold(0.0f64, f64::max);
                        ui.label(
                            RichText::new(format!("{} 峰值", fmt::format_speed(peak)))
                                .font(FontId::new(11.0, egui::FontFamily::Monospace))
                                .color(th.text_faint),
                        );
                    });
                });
                let chart_h = 28.0;
                let (chart_rect, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), chart_h), Sense::hover());
                ui.painter().rect_filled(chart_rect, CornerRadius::same(4), th.inner_panel_bg);
                let max_speed = self
                    .speed_history
                    .iter()
                    .cloned()
                    .fold(1024.0 * 1024.0 * 5.0, f64::max);
                let bw = (chart_rect.width() - 12.0) / SPEED_HISTORY_CAP as f32;
                for (i, v) in self.speed_history.iter().enumerate() {
                    let h = ((v / max_speed).clamp(0.08, 1.0) * (chart_h - 6.0) as f64) as f32;
                    let x = chart_rect.left() + 6.0 + i as f32 * bw;
                    let br = egui::Rect::from_min_size(
                        Pos2::new(x, chart_rect.bottom() - 3.0 - h),
                        Vec2::new(bw - 1.0, h),
                    );
                    ui.painter().rect_filled(br, CornerRadius::same(1), th.accent.gamma_multiply(0.7));
                }
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            // 操作按钮组：左侧主控 / 右侧辅助（任务记录与设置从标题栏移入）
            ui.horizontal(|ui| {
                let (primary_label, primary_kind) = if self.running && !self.paused {
                    ("暂停", Btn::Amber)
                } else if self.paused {
                    ("继续", Btn::Primary)
                } else {
                    ("开始下载", Btn::Primary)
                };
                let primary_icon = if self.running && !self.paused { Icon::Pause } else { Icon::Play };
                if widgets::button(ui, &th, Some(primary_icon), primary_label, &primary_kind, true, ACTION_BTN)
                    .clicked()
                {
                    self.on_primary_click();
                }
                if widgets::button(ui, &th, Some(Icon::Stop), "取消", &Btn::Neutral, self.running, ACTION_BTN)
                    .clicked()
                {
                    self.cancel_download();
                }
                if self.outcome == Outcome::Completed && !self.running {
                    if widgets::button(ui, &th, Some(Icon::RotateCcw), "重下", &Btn::Neutral, true, ACTION_BTN)
                        .clicked()
                    {
                        self.start_download();
                    }
                }
                if widgets::chip(ui, &th, "下载完打开目录", self.auto_open).clicked() {
                    self.auto_open = !self.auto_open;
                    self.config.auto_open = self.auto_open;
                    self.save_config();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::button(ui, &th, Some(Icon::FolderCheck), "打开目录", &Btn::Neutral, true, ACTION_BTN)
                        .clicked()
                    {
                        let dir = self.config.save_dir.clone();
                        self.open_dir(&dir);
                    }
                    if widgets::button(ui, &th, Some(Icon::Sparkles), "直链下载", &Btn::EmeraldSoft, true, ACTION_BTN)
                        .clicked()
                    {
                        self.download_directly();
                    }
                });
            });
        });
    }

    fn log_card(&mut self, ui: &mut egui::Ui) {
        let th = self.th;
        let mono12 = FontId::monospace(12.0);
        // 完全自绘：卡片底板占满容器（减去底部外边距 10），黑框占内容区剩余全部，
        // 高度由容器矩形决定，无任何“填满 available”推导（Frame 的 margin
        // 在子区域 max_rect 中不扣减，靠 Frame + set_min_height 填充必然溢出）
        let full = egui::Rect::from_min_max(
            ui.max_rect().left_top(),
            Pos2::new(ui.max_rect().right(), ui.max_rect().bottom() - 10.0),
        );
        ui.painter().rect_filled(full, CornerRadius::same(12), th.card_bg);
        ui.painter().rect_stroke(
            full,
            CornerRadius::same(12),
            Stroke::new(1.0, th.card_border),
            StrokeKind::Middle,
        );

        // 内容区（内边距 14）
        let mut c = ui.new_child(UiBuilder::new().max_rect(full.shrink(14.0)));
        c.spacing_mut().item_spacing.y = 0.0;

        widgets::section_header(&mut c, &th, Icon::Terminal, th.text_muted, "运行日志", None, |ui| {
            ui.label(
                RichText::new(format!("({})", self.logs.len()))
                    .font(FontId::new(11.0, egui::FontFamily::Monospace))
                    .color(th.text_faint),
            );
            if widgets::chip(ui, &th, "滚屏", self.autoscroll).clicked() {
                self.autoscroll = !self.autoscroll;
            }
            if widgets::chip(ui, &th, "导出", false).clicked() {
                self.export_log();
            }
            if widgets::chip(ui, &th, "复制", false).clicked() {
                let lines: Vec<String> = self
                    .logs
                    .iter()
                    .filter(|(_, l, m)| self.log_passes(*l, m))
                    .map(|(ts, l, m)| format!("{ts} [{}] {m}", l.as_str()))
                    .collect();
                if !lines.is_empty() {
                    ui.ctx().copy_text(lines.join("
"));
                    let n = lines.len();
                    self.push_log(LogLevel::Info, format!("已复制 {n} 条日志到剪贴板"));
                }
            }
            if widgets::chip(ui, &th, "清空", false).clicked() {
                self.logs.clear();
            }
            egui::ComboBox::from_id_salt("log_level")
                .selected_text(self.log_filter.clone())
                .width(84.0)
                .show_ui(ui, |ui| {
                    for level in LOG_LEVELS {
                        ui.selectable_value(&mut self.log_filter, level.to_string(), level);
                    }
                });
            ui.add(
                TextEdit::singleline(&mut self.log_search)
                    .hint_text("搜索...")
                    .desired_width(110.0),
            );
        });
        c.add_space(4.0);

        let filtered: Vec<usize> = self
            .logs
            .iter()
            .enumerate()
            .filter(|(_, (_, l, m))| self.log_passes(*l, m))
            .map(|(i, _)| i)
            .collect();
        let count = filtered.len();
        let row_h = 19.0;

        // 黑框：内容区剩余全部（精确，无推导）
        let black = egui::Rect::from_min_max(
            c.cursor().left_top(),
            Pos2::new(c.max_rect().right(), c.max_rect().bottom()),
        );
        c.painter().rect_filled(black, CornerRadius::same(8), th.log_bg);
        c.painter().rect_stroke(
            black,
            CornerRadius::same(8),
            Stroke::new(1.0, th.log_border),
            StrokeKind::Middle,
        );
        let mut sc_child = c.new_child(UiBuilder::new().max_rect(black.shrink(10.0)));
        sc_child.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical()
            .id_salt("log_scroll")
            .auto_shrink(false)
            .stick_to_bottom(self.autoscroll)
            .show(&mut sc_child, |ui| {
                for row in 0..count {
                    let (ts, level, msg) = &self.logs[filtered[row]];
                    ui.horizontal_wrapped(|ui| {
                        ui.set_min_height(row_h);
                        ui.label(RichText::new(ts).font(mono12.clone()).color(th.log_ts));
                        ui.label(
                            RichText::new(format!("[{}]", level.as_str()))
                                .font(mono12.clone())
                                .strong()
                                .color(theme::log_color(&th, *level)),
                        );
                        ui.label(RichText::new(msg).font(mono12.clone()).color(th.log_fg));
                    });
                }
            });
    }

    fn export_log(&mut self) {
        let lines: Vec<String> = self
            .logs
            .iter()
            .filter(|(_, l, m)| self.log_passes(*l, m))
            .map(|(ts, l, m)| format!("{ts} [{}] {m}", l.as_str()))
            .collect();
        if lines.is_empty() {
            self.push_log(LogLevel::Info, "当前没有可导出的日志");
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("github-downloader-log.txt")
            .save_file()
        {
            match std::fs::write(&path, lines.join("\n")) {
                Ok(_) => {
                    let n = lines.len();
                    self.push_log(LogLevel::Success, format!("日志已导出: {} ({n} 条)", path.display()));
                }
                Err(e) => self.push_log(LogLevel::Error, format!("日志导出失败: {e}")),
            }
        }
    }

    // ── 历史弹窗 ──

    fn history_window(&mut self, ui: &mut egui::Ui) {
        let th = self.th;
        let ctx = ui.ctx().clone();
        egui::Window::new("下载历史记录")
            .id(egui::Id::new("history_modal_v2"))
            .title_bar(false)
            .resizable(true)
            .collapsible(false)
            .default_width(520.0)
            .default_height(400.0)
            .frame(
                egui::Frame::default()
                    .fill(th.card_bg)
                    .stroke(Stroke::new(1.0, th.card_border))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::ZERO),
            )
            .show(&ctx, |ui| {
                ui.set_min_width(520.0);
                // 头部：固定 44px 高，内容垂直居中
                let (hdr, _) = ui
                    .allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::hover());
                let mut head = ui.new_child(UiBuilder::new().max_rect(hdr));
                head.horizontal_centered(|ui| {
                    ui.add_space(16.0);
                    ui.label(RichText::new("下载历史记录").strong().size(14.0).color(th.text));
                    ui.label(
                        RichText::new(format!("({} 个任务)", self.config.history.len()))
                            .font(FontId::new(11.0, egui::FontFamily::Monospace))
                            .color(th.text_faint),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(10.0);
                        if widgets::titlebar_icon_btn(ui, &th, Icon::Close, "关闭", th.hover).clicked() {
                            self.show_history = false;
                        }
                        ui.add_space(4.0);
                        if !self.config.history.is_empty()
                            && widgets::chip(ui, &th, "清空记录", false).clicked()
                        {
                            self.config.history.clear();
                            self.save_config();
                            self.push_log(LogLevel::Info, "已清空任务历史记录");
                        }
                    });
                });
                ui.separator();
                // 列表
                egui::Frame::default().inner_margin(Margin::same(16)).show(ui, |ui| {
                    if self.config.history.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(32.0);
                            ui.label(
                                RichText::new("暂无已完成的下载任务")
                                    .size(13.0)
                                    .color(th.text_faint),
                            );
                            ui.add_space(32.0);
                        });
                    }
                    let mut redownload: Option<usize> = None;
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (idx, item) in self.config.history.iter().enumerate().rev() {
                            egui::Frame::default()
                                .fill(th.inner_panel_bg)
                                .stroke(Stroke::new(1.0, th.card_border))
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(Margin::same(10))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        crate::ui::icons::show(ui, Icon::Package, 14.0, th.accent_text, th.card_bg);
                                        ui.label(
                                            RichText::new(&item.name)
                                                .font(FontId::monospace(13.0))
                                                .color(if item.ok { th.text_secondary } else { th.rose }),
                                        );
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if widgets::chip(ui, &th, "重下", false).clicked() {
                                                redownload = Some(idx);
                                            }
                                            ui.label(
                                                RichText::new(if item.ok { "成功" } else { "失败" })
                                                    .size(12.0)
                                                    .color(if item.ok { th.emerald_text } else { th.rose }),
                                            );
                                        });
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(format!(
                                                "{} · {} · {}",
                                                if item.size > 0 {
                                                    fmt::format_size(item.size)
                                                } else {
                                                    "--".into()
                                                },
                                                item.time,
                                                item.save_path
                                            ))
                                            .font(FontId::new(11.0, egui::FontFamily::Monospace))
                                            .color(th.text_faint),
                                        );
                                    });
                                });
                            ui.add_space(6.0);
                        }
                    });
                    if let Some(idx) = redownload {
                        if let Some(item) = self.config.history.get(idx) {
                            let url = item.url.clone();
                            let name = item.name.clone();
                            self.url_input = url;
                            self.filename = name;
                            self.show_history = false;
                            self.parse_url(false);
                            self.start_download();
                        }
                    }
                });
                ui.separator();
                // 底部
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(16.0);
                        if widgets::button(ui, &th, None, "关闭", &Btn::Neutral, true, Vec2::new(76.0, 30.0))
                            .clicked()
                        {
                            self.show_history = false;
                        }
                    });
                });
                ui.add_space(10.0);
            });
    }

    // ── 设置弹窗 ──

    fn settings_window(&mut self, ui: &mut egui::Ui) {
        let th = self.th;
        let ctx = ui.ctx().clone();
        egui::Window::new("系统首选项与加速配置")
            .id(egui::Id::new("settings_modal_v2"))
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .default_width(460.0)
            .frame(
                egui::Frame::default()
                    .fill(th.card_bg)
                    .stroke(Stroke::new(1.0, th.card_border))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::ZERO),
            )
            .show(&ctx, |ui| {
                ui.set_min_width(460.0);
                // 头部：固定 44px 高，内容垂直居中
                let (hdr, _) = ui
                    .allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::hover());
                let mut head = ui.new_child(UiBuilder::new().max_rect(hdr));
                head.horizontal_centered(|ui| {
                    ui.add_space(16.0);
                    crate::ui::icons::show(ui, Icon::Settings, 15.0, th.accent_text, th.card_bg);
                    ui.label(RichText::new("系统首选项与加速配置").strong().size(14.0).color(th.text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(10.0);
                        if widgets::titlebar_icon_btn(ui, &th, Icon::Close, "关闭", th.hover).clicked() {
                            self.show_settings = false;
                        }
                    });
                });
                ui.separator();
                // 主体
                egui::Frame::default().inner_margin(Margin::same(16)).show(ui, |ui| {
                    ui.label(
                        RichText::new("多线程并发分块数 (分段下载):")
                            .size(13.0)
                            .color(th.text_secondary),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let gap = 8.0;
                        ui.spacing_mut().item_spacing.x = gap;
                        let w = (ui.available_width() - gap * 3.0) / 4.0;
                        for t in [4u8, 8, 16, 32] {
                            let selected = self.config.segments == t;
                            let (fill, stroke_c, fg) = if selected {
                                (th.accent, Color32::TRANSPARENT, Color32::WHITE)
                            } else {
                                (th.input_bg, th.input_border, th.text_secondary)
                            };
                            let btn = egui::Button::new(
                                RichText::new(format!("{t} 线程"))
                                    .font(FontId::monospace(13.0))
                                    .color(fg),
                            )
                            .fill(fill)
                            .stroke(Stroke::new(1.0, stroke_c))
                            .corner_radius(CornerRadius::same(8))
                            .min_size(Vec2::new(w, 32.0));
                            if ui.add(btn).clicked() {
                                self.config.segments = t;
                                self.push_log(LogLevel::Info, format!("并发线程调整为: {t} 线程"));
                                self.save_config();
                            }
                        }
                    });
                    ui.label(
                        RichText::new("针对大文件可开启 16 或 32 线程，充分压榨带宽吞吐率。")
                            .size(12.0)
                            .color(th.text_faint),
                    );
                    ui.add_space(14.0);

                    ui.label(
                        RichText::new("网络连接请求超时 (秒):")
                            .size(13.0)
                            .color(th.text_secondary),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let mut t = self.config.timeout_seconds as f32;
                        if ui
                            .add(egui::Slider::new(&mut t, 5.0..=60.0).step_by(5.0).show_value(false))
                            .changed()
                        {
                            self.config.timeout_seconds = t as u32;
                            self.save_config();
                        }
                        ui.label(
                            RichText::new(format!("{}s", self.config.timeout_seconds))
                                .font(FontId::monospace(13.0))
                                .color(th.text_secondary),
                        );
                    });
                    ui.add_space(14.0);
                    ui.separator();
                    ui.add_space(6.0);

                    if ui
                        .checkbox(
                            &mut self.config.auto_clipboard,
                            "剪贴板自动监测 — 复制 GitHub 链接时自动填入并解析",
                        )
                        .changed()
                    {
                        self.auto_clipboard_flag
                            .store(self.config.auto_clipboard, Ordering::Relaxed);
                        self.save_config();
                    }
                    ui.add_space(4.0);
                    if ui
                        .checkbox(
                            &mut self.config.sound_enabled,
                            "下载完成提示音 — 任务完成后播放提示响铃",
                        )
                        .changed()
                    {
                        self.save_config();
                    }
                });
                ui.separator();
                // 底部
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    ui.add_space(6.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(16.0);
                        if widgets::button(ui, &th, None, "保存并关闭", &Btn::Primary, true, Vec2::new(100.0, 30.0))
                            .clicked()
                        {
                            self.show_settings = false;
                        }
                    });
                });
                ui.add_space(10.0);
            });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}

// ────────────────────────── 工具函数 ──────────────────────────

/// Windows 文件名合法性清洗。
fn sanitize_filename(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect();
    let upper = out.to_uppercase();
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
        "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let stem = upper.split('.').next().unwrap_or("");
    if reserved.contains(&stem) {
        out = format!("_{out}");
    }
    while out.ends_with('.') || out.ends_with(' ') {
        out.pop();
    }
    if out.is_empty() {
        out = "downloaded_file".into();
    }
    out
}

#[cfg(windows)]
fn disk_free_bytes(path: &Path) -> Option<u64> {
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            lpDirectoryName: *const u16,
            lpFreeBytesAvailableToCaller: *mut u64,
            lpTotalNumberOfBytes: *mut u64,
            lpTotalNumberOfFreeBytes: *mut u64,
        ) -> i32;
    }
    use std::os::windows::ffi::OsStrExt;
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) };
    (ok != 0).then_some(avail)
}

#[cfg(not(windows))]
fn disk_free_bytes(_path: &Path) -> Option<u64> {
    None
}

/// 下载完成提示音（系统默认提示音；柔和双音需额外音频依赖，暂用系统音替代）。
#[cfg(windows)]
fn play_chime() {
    unsafe extern "system" {
        fn MessageBeep(utype: u32) -> i32;
    }
    unsafe {
        MessageBeep(0); // MB_OK
    }
}

#[cfg(not(windows))]
fn play_chime() {}

/// 加载系统微软雅黑与 Consolas（缺失时回退 egui 内置字体）。
fn load_system_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Ok(data) = std::fs::read("C:\\Windows\\Fonts\\msyh.ttc") {
        fonts
            .font_data
            .insert("msyh".into(), egui::FontData::from_owned(data).into());
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "msyh".into());
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push("msyh".into());
    }
    if let Ok(data) = std::fs::read("C:\\Windows\\Fonts\\consola.ttf") {
        fonts
            .font_data
            .insert("consola".into(), egui::FontData::from_owned(data).into());
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "consola".into());
    }
    ctx.set_fonts(fonts);
}

/// 解码内嵌 ICO 为标题栏图标纹理。
fn load_app_icon(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory_with_format(
        include_bytes!("../assets/app_icon.ico"),
        image::ImageFormat::Ico,
    )
    .ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Some(ctx.load_texture(
        "app_icon",
        egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw()),
        egui::TextureOptions::LINEAR,
    ))
}

/// 主题应用到全局样式：字号体系与常规控件配色。
fn apply_theme(ctx: &egui::Context, th: &Theme) {
    ctx.set_theme(if th.dark {
        egui::ThemePreference::Dark
    } else {
        egui::ThemePreference::Light
    });
    ctx.all_styles_mut(|style| {
        style.visuals.panel_fill = th.window_bg;
        style.visuals.window_fill = th.card_bg;
        style.visuals.extreme_bg_color = th.input_bg;
        style.visuals.selection.bg_fill = th.accent;
        // 输入框边框可见：TextEdit 非聚焦态走 Inactive 视觉
        style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, th.input_border);
        style.visuals.widgets.inactive.bg_fill = th.input_bg;
        style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, th.input_text);
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, th.accent);
        style.visuals.widgets.hovered.bg_fill = th.input_bg;
        style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, th.accent);
        style.visuals.widgets.open.bg_stroke = Stroke::new(1.0, th.accent);
        style.text_styles = [
            (egui::TextStyle::Heading, FontId::proportional(15.0)),
            (egui::TextStyle::Body, FontId::proportional(13.0)),
            (egui::TextStyle::Button, FontId::proportional(13.0)),
            (egui::TextStyle::Small, FontId::proportional(11.0)),
            (egui::TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        style.spacing.item_spacing = Vec2::new(6.0, 4.0);
    });
    // 弹窗底色对两种主题都设置，避免主题切换残留
    ctx.set_visuals_of(egui::Theme::Light, {
        let mut v = egui::Visuals::light();
        v.panel_fill = th.window_bg;
        v.window_fill = th.card_bg;
        v
    });
    ctx.set_visuals_of(egui::Theme::Dark, {
        let mut v = egui::Visuals::dark();
        v.panel_fill = th.window_bg;
        v.window_fill = th.card_bg;
        v
    });
}
