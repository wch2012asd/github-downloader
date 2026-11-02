//! 核心逻辑层：与 GUI 无关，可独立单测。
//!
//! - `url_parser`：GitHub 链接解析
//! - `engines`：加速通道定义、URL 变换与测速
//! - `proxy`：Clash 代理检测
//! - `downloader`：下载状态机（断点续传、分段、重试）
//! - `config`：配置持久化（兼容 v1 的 config.json）
//! - `fmt`：大小/速度/剩余时间格式化

pub mod config;
pub mod downloader;
pub mod engines;
pub mod fmt;
pub mod proxy;
pub mod url_parser;
