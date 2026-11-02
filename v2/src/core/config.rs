//! 配置持久化：EXE 同目录 `config.json`，兼容 v1 字段，原子写入。

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const DEFAULT_PROXY_PORT: u16 = 7890;
pub const DEFAULT_GHPROXY_PREFIX: &str = "https://ghproxy.net/";
pub const DEFAULT_KKGITHUB_HOST: &str = "https://kkgithub.com";
pub const DEFAULT_MOEYY_PREFIX: &str = "https://github.moeyy.xyz/";
pub const MAX_HISTORY: usize = 30;

/// 一条任务记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    #[serde(default)]
    pub time: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub save_path: String,
}

/// 程序配置；字段缺省值与 v1 兼容（v1 的 config.json 可直接读取）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub proxy_port: u16,
    pub save_dir: String,
    pub engine: String,
    pub auto_open: bool,
    pub history: Vec<HistoryEntry>,
    pub ghproxy_prefix: String,
    pub kkgithub_host: String,
    /// 分段下载并发数（v2 新增）
    pub segments: u8,
    /// 深色主题（设计稿 v2.4 新增）
    pub theme_dark: bool,
    /// 剪贴板自动监测
    pub auto_clipboard: bool,
    /// 下载完成提示音
    pub sound_enabled: bool,
    /// 网络连接请求超时（秒）
    pub timeout_seconds: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            proxy_port: DEFAULT_PROXY_PORT,
            save_dir: String::new(),
            engine: "clash".into(),
            auto_open: false,
            history: Vec::new(),
            ghproxy_prefix: DEFAULT_GHPROXY_PREFIX.into(),
            kkgithub_host: DEFAULT_KKGITHUB_HOST.into(),
            segments: 8,
            theme_dark: false,
            auto_clipboard: false,
            sound_enabled: true,
            timeout_seconds: 20,
        }
    }
}

impl Config {
    /// 校正越界字段（分段数 1..=32 等）。
    pub fn sanitized(mut self) -> Self {
        self.segments = self.segments.clamp(1, 32);
        if self.save_dir.is_empty() {
            self.save_dir = std::env::current_dir()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
        self
    }
}

/// 配置文件所在目录：EXE 同目录（开发期即 v2/target/debug，生产为发布目录）。
pub fn config_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_path_buf)
}

pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("config.json"))
}

/// 从 EXE 同目录读取配置；文件缺失或损坏时返回默认值。
pub fn load() -> Config {
    let path = config_path().unwrap_or_else(|| PathBuf::from("config.json"));
    load_from(&path).sanitized()
}

pub fn load_from(path: &Path) -> Config {
    fs::read(path)
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default()
}

/// 保存到 EXE 同目录；先写临时文件再 rename，避免中途被杀导致配置损坏。
pub fn save(config: &Config) -> bool {
    let path = config_path().unwrap_or_else(|| PathBuf::from("config.json"));
    save_to(&path, config)
}

pub fn save_to(path: &Path, config: &Config) -> bool {
    let Ok(json) = serde_json::to_vec_pretty(config) else {
        return false;
    };
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, &json).is_err() {
        return false;
    }
    fs::rename(&tmp, path).is_ok()
}

/// 追加一条历史并裁剪到上限；返回新的历史列表。
pub fn push_history(history: &mut Vec<HistoryEntry>, entry: HistoryEntry) {
    history.push(entry);
    if history.len() > MAX_HISTORY {
        let drop = history.len() - MAX_HISTORY;
        history.drain(..drop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("ghdl-test-{tag}-{}.json", std::process::id()));
        let _ = fs::remove_file(&p);
        p
    }

    #[test]
    fn roundtrip() {
        let path = tmp_path("roundtrip");
        let mut cfg = Config::default();
        cfg.proxy_port = 7897;
        cfg.save_dir = "D:/Downloads".into();
        cfg.engine = "ghproxy".into();
        cfg.auto_open = true;
        cfg.segments = 16;
        push_history(
            &mut cfg.history,
            HistoryEntry {
                time: "2026-10-13 12:00".into(),
                name: "a.zip".into(),
                size: 123,
                url: "https://x".into(),
                ok: true,
                save_path: "D:/Downloads".into(),
            },
        );
        assert!(save_to(&path, &cfg));
        let loaded = load_from(&path);
        assert_eq!(loaded.proxy_port, 7897);
        assert_eq!(loaded.engine, "ghproxy");
        assert_eq!(loaded.segments, 16);
        assert_eq!(loaded.history.len(), 1);
        assert_eq!(loaded.history[0].name, "a.zip");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn reads_v1_config() {
        let path = tmp_path("v1");
        // v1 真实格式：只有部分字段，且没有 v2 的 segments
        fs::write(&path, r#"{"proxy_port": 7897, "engine": "clash"}"#).unwrap();
        let cfg = load_from(&path);
        assert_eq!(cfg.proxy_port, 7897);
        assert_eq!(cfg.segments, 8); // 新字段取默认
        assert_eq!(cfg.ghproxy_prefix, DEFAULT_GHPROXY_PREFIX);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn broken_file_returns_default() {
        let path = tmp_path("broken");
        fs::write(&path, "{not json").unwrap();
        let cfg = load_from(&path);
        assert_eq!(cfg.proxy_port, DEFAULT_PROXY_PORT);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn history_capped_at_30() {
        let mut h = Vec::new();
        for i in 0..40 {
            push_history(
                &mut h,
                HistoryEntry {
                    time: String::new(),
                    name: format!("f{i}"),
                    size: 0,
                    url: String::new(),
                    ok: true,
                    save_path: String::new(),
                },
            );
        }
        assert_eq!(h.len(), MAX_HISTORY);
        assert_eq!(h[0].name, "f10"); // 最旧的被裁掉
        assert_eq!(h.last().unwrap().name, "f39");
    }
}
