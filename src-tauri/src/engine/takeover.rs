//! 浏览器接管：本地 HTTP 接口（127.0.0.1，token 鉴权）。
//!
//! 配套浏览器扩展（browser-extension/）监听浏览器下载并转发到这里；
//! 接口每请求读取设置（启用开关/token 即时生效），监听端口改动需重启应用。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};

use crate::store::{NewTask, TaskKind};
use crate::AppState;

/// 从设置生成默认 token（首启自动生成并持久化）。
pub async fn ensure_token(state: &Arc<AppState>) -> String {
    if let Some(t) = state
        .store
        .get_setting("browser_takeover_token")
        .await
        .ok()
        .flatten()
        .filter(|t| !t.is_empty())
    {
        return t;
    }
    let t = uuid::Uuid::new_v4().to_string();
    let _ = state.store.set_setting("browser_takeover_token", &t).await;
    t
}

/// 启动接管监听（应用启动时调用一次；端口改动需重启）。
pub async fn run_listener(state: Arc<AppState>, port: u16) -> std::io::Result<()> {
    ensure_token(&state).await;
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            break;
        };
        let st = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_conn(st, stream).await {
                eprintln!("[takeover] 请求处理失败: {e}");
            }
        });
    }
    Ok(())
}

async fn handle_conn(state: Arc<AppState>, stream: TcpStream) -> std::io::Result<()> {
    let (mut reader, mut writer) = stream.into_split();
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let mut buf = tokio::io::BufReader::new(&mut reader);
    let mut first = String::new();
    buf.read_line(&mut first).await?;
    // 读完头部（忽略字段本身，只需要 Content-Length 与 token 头）
    let mut content_length = 0usize;
    let mut token_ok = false;
    loop {
        let mut line = String::new();
        if buf.read_line(&mut line).await? == 0 {
            break;
        }
        let t = line.trim_end();
        if t.is_empty() {
            break;
        }
        let lower = t.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        }
        if let Some(v) = lower.strip_prefix("x-token:") {
            let want = state
                .store
                .get_setting("browser_takeover_token")
                .await
                .ok()
                .flatten()
                .unwrap_or_default();
            token_ok = v.trim() == want && !want.is_empty();
        }
    }
    let mut body = vec![0u8; content_length];
    buf.read_exact(&mut body).await?;

    let is_post = first.starts_with("POST ");
    let path = first.split_whitespace().nth(1).unwrap_or("");
    let (status, payload) = if !is_post || !token_ok {
        (
            "403 Forbidden",
            r#"{"ok":false,"error":"未授权的接管请求"}"#.to_string(),
        )
    } else {
        let outcome = if path.starts_with("/api/upload") {
            handle_upload(state, path, body).await
        } else {
            handle_download(state, &String::from_utf8_lossy(&body)).await
        };
        match outcome {
            Ok(id) => ("200 OK", format!(r#"{{"ok":true,"id":"{id}"}}"#)),
            Err(e) => (
                "400 Bad Request",
                format!(r#"{{"ok":false,"error":{}}}"#, serde_json::to_string(&e).unwrap_or_default()),
            ),
        }
    };

    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    writer.write_all(head.as_bytes()).await?;
    writer.write_all(payload.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

/// 处理接管下载：落库入队（保存目录/线程数走设置默认）。
async fn handle_download(state: Arc<AppState>, body: &str) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Payload {
        url: String,
        #[serde(default)]
        filename: Option<String>,
    }
    let p: Payload =
        serde_json::from_str(body).map_err(|e| format!("请求体不是合法 JSON: {e}"))?;
    if p.url.trim().is_empty() {
        return Err("url 不能为空".into());
    }
    let save_dir = save_dir_of(&state).await;
    let segments = state
        .store
        .get_setting("segments")
        .await
        .ok()
        .flatten()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(crate::core::config::DEFAULT_SEGMENTS as u32);
    // onCreated 时刻 filename 可能是空串或完整绝对路径：
    // 空串回退 URL 推导；绝对路径只取文件名段，否则 join 会覆盖 save_dir
    let name = p
        .filename
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|s| Path::new(s).file_name())
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            p.url
                .split('?')
                .next()
                .unwrap_or(&p.url)
                .rsplit('/')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("downloaded_file")
                .to_string()
        });

    if !takeover_enabled(&state).await {
        return Err("浏览器接管已在设置中关闭".into());
    }

    state
        .manager
        .enqueue(NewTask {
            kind: TaskKind::Http,
            name,
            save_dir,
            category: String::new(),
            urls: vec![p.url],
            segments: Some(segments),
            limit_bps: None,
            use_clash: false,
        })
        .await
        .map_err(|e| e.to_string())
}

/// 处理 blob 等页面内存文件的上传：字节已随请求体送达，直接落盘并补一条已完成记录。
async fn handle_upload(state: Arc<AppState>, path: &str, body: Vec<u8>) -> Result<String, String> {
    if !takeover_enabled(&state).await {
        return Err("浏览器接管已在设置中关闭".into());
    }
    let raw = path
        .split_once('?')
        .map(|(_, q)| q)
        .unwrap_or_default()
        .split('&')
        .find_map(|kv| kv.strip_prefix("name="))
        .map(percent_decode)
        .unwrap_or_default();
    // 只取文件名段，防止路径穿越写入任意位置
    let name = Path::new(&raw)
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .ok_or_else(|| "文件名无效".to_string())?
        .to_string();

    let mut task = state
        .manager
        .apply_classification(NewTask {
            kind: TaskKind::Http,
            name,
            save_dir: save_dir_of(&state).await,
            category: String::new(),
            urls: Vec::new(),
            segments: None,
            limit_bps: None,
            use_clash: false,
        })
        .await;
    let (name, target) = unique_path(&task.save_dir, &task.name);
    task.name = name;

    tokio::fs::write(&target, &body)
        .await
        .map_err(|e| format!("写入文件失败: {e}"))?;

    let size = body.len() as i64;
    let id = state.store.insert_task(&task).await.map_err(|e| e.to_string())?;
    let _ = state.store.update_progress(&id, size, Some(size)).await;
    state.manager.mark_completed(&id).await;
    Ok(id)
}

/// 保存目录（设置 save_dir，缺省 D:\Downloads）。
async fn save_dir_of(state: &Arc<AppState>) -> String {
    state
        .store
        .get_setting("save_dir")
        .await
        .ok()
        .flatten()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "D:\\Downloads".into())
}

/// 接管总开关（设置 browser_takeover_enabled，缺省开）。
async fn takeover_enabled(state: &Arc<AppState>) -> bool {
    state
        .store
        .get_setting("browser_takeover_enabled")
        .await
        .ok()
        .flatten()
        .and_then(|s| s.parse::<bool>().ok())
        .unwrap_or(true)
}

/// 目标文件重名时依次尝试 "名 (1).ext"、"名 (2).ext"…
fn unique_path(dir: &str, name: &str) -> (String, PathBuf) {
    let base = Path::new(dir);
    let direct = base.join(name);
    if !direct.exists() {
        return (name.to_string(), direct);
    }
    let src = Path::new(name);
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = src.extension().and_then(|e| e.to_str());
    for n in 1..1000 {
        let cand = match ext {
            Some(e) => format!("{stem} ({n}).{e}"),
            None => format!("{stem} ({n})"),
        };
        let p = base.join(&cand);
        if !p.exists() {
            return (cand, p);
        }
    }
    (name.to_string(), direct)
}

/// 百分号解码（encodeURIComponent 的输出；非法序列原样保留）。
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// 启动监听所需的端口（设置 browser_takeover_port，默认 24556）。
pub async fn takeover_port(state: &Arc<AppState>) -> u16 {
    state
        .store
        .get_setting("browser_takeover_port")
        .await
        .ok()
        .flatten()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(24556)
}
