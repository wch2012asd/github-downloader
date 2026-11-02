//! GUI 与后台线程的事件契约：后台只发 `UiEvent`，UI 每帧排空队列。


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Success,
    Error,
    /// 代理/通道相关事件（设计稿新增级别，青蓝色显示）
    Proxy,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Success => "SUCCESS",
            LogLevel::Error => "ERROR",
            LogLevel::Proxy => "PROXY",
        }
    }
}

/// 一次进度采样；`total` 为 0 表示大小未知（只显示已传输）。
#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)] // total 供核心层与后续界面使用
pub struct Progress {
    pub downloaded: u64,
    pub total: u64,
    pub speed_bps: f64,
    pub pct: f64,
}

#[derive(Debug)]
pub enum UiEvent {
    Log {
        level: LogLevel,
        msg: String,
    },
    Progress {
        task_gen: u64,
        progress: Progress,
    },
    TaskFinished {
        task_gen: u64,
        ok: bool,
        reason: String,
    },
    SizeFetched {
        task_gen: u64,
        size: Option<u64>,
    },
    ProxyCheck {
        port: Option<u16>,
        latency: Option<u32>,
    },
    /// 手动「测试连接」的结果；失败只提示，不改变在线状态
    ProxyTest {
        port: u16,
        latency: Option<u32>,
    },
    ChannelLatency {
        id: &'static str,
        latency: Option<u32>,
    },
    /// 剪贴板监测发现的新 GitHub 链接
    ClipboardLink { text: String },
}

