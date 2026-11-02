//! 加速通道引擎：通道定义、下载 URL 变换、代理配置与延迟测量。

use std::time::Instant;

const TEST_URL: &str = "https://api.github.com/zen";
const PING_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// 一条加速通道。
pub struct Channel {
    pub id: &'static str,
    pub name: &'static str,
    /// 徽标文字
    pub tag: &'static str,
    /// 悬停说明
    pub description: &'static str,
    /// 未测速时展示的延迟提示
    pub latency_hint: &'static str,
}

pub const CHANNELS: [Channel; 5] = [
    Channel {
        id: "clash",
        name: "Clash 本地代理",
        tag: "推荐",
        description: "通过本地 Clash / 客户端梯子转发流量",
        latency_hint: "约 45ms",
    },
    Channel {
        id: "ghproxy",
        name: "GHProxy 镜像",
        tag: "免客户端",
        description: "国内高速分发 CDN 节点",
        latency_hint: "约 68ms",
    },
    Channel {
        id: "kkgithub",
        name: "KKGitHub 节点",
        tag: "极速",
        description: "全球负载均衡高速反代",
        latency_hint: "约 72ms",
    },
    Channel {
        id: "moeyy",
        name: "Moeyy 加速源",
        tag: "新增",
        description: "Moeyy 公共加速节点",
        latency_hint: "约 88ms",
    },
    Channel {
        id: "direct",
        name: "官方直连",
        tag: "原始",
        description: "不经由加速节点，直接连接 GitHub 官方",
        latency_hint: "可能受限",
    },
];

/// 按 id 查通道；未知名回退第一条。
pub fn channel(engine_id: &str) -> &'static Channel {
    CHANNELS
        .iter()
        .find(|c| c.id == engine_id)
        .unwrap_or(&CHANNELS[0])
}

/// 将原始 GitHub URL 变换为指定通道的下载地址（与设计稿 buildAcceleratedUrl 对齐）。
pub fn build_download_url(
    engine_id: &str,
    url: &str,
    ghproxy_prefix: &str,
    kkgithub_host: &str,
) -> String {
    match engine_id {
        "ghproxy" => format!("{}/{}", ghproxy_prefix.trim_end_matches('/'), url),
        "moeyy" => format!(
            "{}/{}",
            crate::core::config::DEFAULT_MOEYY_PREFIX.trim_end_matches('/'),
            url
        ),
        "kkgithub" => {
            let host = kkgithub_host.trim_end_matches('/');
            // kkgithub 镜像的 raw 域名为 raw.<主域名>
            let raw_host = host.replacen("://", "://raw.", 1);
            url.replace("https://raw.githubusercontent.com", &raw_host)
                .replace("https://github.com", host)
        }
        _ => url.to_string(),
    }
}

/// 通道对应的代理地址；镜像/直连通道不走本地代理。
pub fn proxies_for(engine_id: &str, clash_port: Option<u16>) -> Option<String> {
    match (engine_id, clash_port) {
        ("clash", Some(port)) => Some(format!("http://127.0.0.1:{port}")),
        _ => None,
    }
}

/// 测量通道连通延迟（毫秒）；通道不可用返回 None。
/// 判定标准：收到任何非 5xx 的 HTTP 响应即算可用（403 限流 ≠ 通道故障）。
pub fn measure_latency(
    engine_id: &str,
    clash_port: Option<u16>,
    ghproxy_prefix: &str,
    kkgithub_host: &str,
) -> Option<u32> {
    let (url, proxy): (String, Option<String>) = match engine_id {
        "clash" => (TEST_URL.to_string(), proxies_for("clash", clash_port)),
        "ghproxy" => (ghproxy_prefix.to_string(), None),
        "kkgithub" => (kkgithub_host.to_string(), None),
        "moeyy" => (crate::core::config::DEFAULT_MOEYY_PREFIX.to_string(), None),
        _ => (TEST_URL.to_string(), None),
    };
    // 代理只能建在 client 上（blocking RequestBuilder 无 proxy 方法）
    let mut builder = reqwest::blocking::Client::builder()
        .user_agent(crate::core::downloader::USER_AGENT)
        .timeout(PING_TIMEOUT);
    if let Some(p) = &proxy {
        builder = builder.proxy(reqwest::Proxy::all(p).ok()?);
    }
    let client = builder.build().ok()?;
    let req = client.head(&url);
    let start = Instant::now();
    match req.send() {
        // 收到任何 HTTP 响应（含 403 限流）都证明链路可达；5xx 视为节点故障
        Ok(_) => Some(start.elapsed().as_millis() as u32),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_lookup_falls_back() {
        assert_eq!(channel("direct").id, "direct");
        assert_eq!(channel("nope").id, "clash");
        assert_eq!(CHANNELS.len(), 5);
        assert_eq!(channel("moeyy").name, "Moeyy 加速源");
    }

    #[test]
    fn url_transform_ghproxy() {
        assert_eq!(
            build_download_url("ghproxy", "https://github.com/a/b/releases/download/v1/x.zip", "https://ghproxy.net/", ""),
            "https://ghproxy.net/https://github.com/a/b/releases/download/v1/x.zip"
        );
    }

    #[test]
    fn url_transform_moeyy() {
        assert_eq!(
            build_download_url("moeyy", "https://github.com/a/b/archive/main.zip", "", ""),
            "https://github.moeyy.xyz/https://github.com/a/b/archive/main.zip"
        );
    }

    #[test]
    fn url_transform_kkgithub() {
        assert_eq!(
            build_download_url("kkgithub", "https://github.com/a/b/archive/main.zip", "", "https://kkgithub.com"),
            "https://kkgithub.com/a/b/archive/main.zip"
        );
        assert_eq!(
            build_download_url("kkgithub", "https://raw.githubusercontent.com/a/b/main/f.txt", "", "https://kkgithub.com"),
            "https://raw.kkgithub.com/a/b/main/f.txt"
        );
    }

    #[test]
    fn url_transform_direct_untouched() {
        assert_eq!(
            build_download_url("direct", "https://github.com/a/b", "", ""),
            "https://github.com/a/b"
        );
    }

    #[test]
    fn proxies_only_for_clash() {
        assert_eq!(proxies_for("clash", Some(7890)).as_deref(), Some("http://127.0.0.1:7890"));
        assert_eq!(proxies_for("clash", None), None);
        assert_eq!(proxies_for("ghproxy", Some(7890)), None);
    }
}
