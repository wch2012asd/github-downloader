//! 核心逻辑层：与界面无关，可独立单测（v3 起挂载于 src-tauri）。
//!
//! - `url_parser`：GitHub 链接解析
//! - `engines`：加速通道定义、URL 变换与测速
//! - `proxy`：Clash 代理检测
//! - `config`：配置持久化（兼容 v1/v2 的 config.json）
//! - `fmt`：大小/速度/剩余时间格式化
//! - `downloader`：C3 阶段按 tokio async 重写后接入（v2 版本作移植规范）

/// HTTP 客户端统一 UA（原 v2 `downloader::USER_AGENT`；测速与下载共用）。
pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) VortexDL/3.0";

pub mod classify;
pub mod config;
pub mod engines;
pub mod fmt;
pub mod proxy;
pub mod url_parser;
