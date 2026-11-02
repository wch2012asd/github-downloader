//! Clash 代理检测：注册表读系统代理端口 + 常见端口扫描 + 经代理实测 GitHub。

use std::net::TcpStream;
use std::time::{Duration, Instant};

use winreg::enums::{HKEY_CURRENT_USER, REG_SZ};
use winreg::RegKey;

pub const CLASH_COMMON_PORTS: [u16; 9] = [7890, 7897, 7891, 7892, 7899, 10809, 1080, 8080, 2080];
// 用 github.com 主站验证（api.github.com 有 IP 限流，403 会被误判为代理故障）
const TEST_URL: &str = "https://github.com/";
const TEST_TIMEOUT: Duration = Duration::from_secs(8);
const CONNECT_PROBE_TIMEOUT: Duration = Duration::from_secs(1);

/// TCP 探活。
pub fn port_open(port: u16, host: &str) -> bool {
    let addr = format!("{host}:{port}");
    // 解析一次即可；connect_timeout 限制等待时长
    match std::net::ToSocketAddrs::to_socket_addrs(&addr) {
        Ok(mut addrs) => match addrs.next() {
            Some(sock) => TcpStream::connect_timeout(&sock, CONNECT_PROBE_TIMEOUT).is_ok(),
            None => false,
        },
        Err(_) => false,
    }
}

fn test_client(port: u16, host: &str) -> Option<reqwest::blocking::Client> {
    let proxy_url = format!("http://{host}:{port}");
    let proxy = reqwest::Proxy::all(&proxy_url).ok()?;
    reqwest::blocking::Client::builder()
        .timeout(TEST_TIMEOUT)
        .proxy(proxy)
        .build()
        .ok()
}

/// 通过指定代理请求 GitHub，返回延迟毫秒；代理不可用返回 None。
/// 判定标准：收到任何非 5xx 的 HTTP 响应即视为链路可用
///（GitHub 对受限 IP 可能返回 403，但那说明代理链路本身是通的）。
pub fn ping_proxy(port: u16, host: &str) -> Option<u32> {
    let client = test_client(port, host)?;
    let start = Instant::now();
    let resp = client.get(TEST_URL).send().ok()?;
    if resp.status().is_server_error() {
        return None; // 5xx 通常是节点上游故障
    }
    Some(start.elapsed().as_millis() as u32)
}

/// 读取 Windows 系统代理端口（Clash 开启系统代理时写入注册表）；未开启返回 None。
pub fn system_proxy_port() -> Option<u16> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
        .ok()?;
    let enabled: u32 = key.get_value("ProxyEnable").ok()?;
    let server: String = match key.get_value("ProxyServer") {
        Ok(v) => v,
        Err(_) => {
            // QueryValueEx 泛型失败可能因类型不是 REG_SZ；显式按字符串再试一次
            key.get_value::<String, _>("ProxyServer").ok()?
        }
    };
    let _ = REG_SZ; // 文档性引用：期望的注册表类型
    if enabled == 0 {
        return None;
    }
    // ProxyServer 可能是 "127.0.0.1:7890"，也可能是 "http=...;https=...;socks=..." 形式
    server.split(';').find_map(|part| {
        let addr = part.split('=').next_back().unwrap_or("").trim();
        addr.rsplit(':').next()?.parse::<u16>().ok()
    })
}

/// 检测 Clash：系统代理端口优先，失败再扫常见端口；返回 (端口, 延迟)。
pub fn detect_clash() -> Option<(u16, Option<u32>)> {
    let mut candidates: Vec<u16> = Vec::new();
    if let Some(p) = system_proxy_port() {
        candidates.push(p);
    }
    candidates.extend(CLASH_COMMON_PORTS);
    let mut tried = std::collections::HashSet::new();
    for port in candidates {
        if !tried.insert(port) {
            continue;
        }
        if port_open(port, "127.0.0.1") {
            if let Some(latency) = ping_proxy(port, "127.0.0.1") {
                return Some((port, Some(latency)));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_open_rejects_closed() {
        // 用一个大概率没人监听的端口
        assert!(!port_open(1, "127.0.0.1"));
    }

    #[test]
    fn system_proxy_port_never_panics() {
        // 无论本机注册表状态如何都只返回 Option
        let _ = system_proxy_port();
    }
}
