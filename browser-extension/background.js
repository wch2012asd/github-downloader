// 监听浏览器所有下载 → 转交本机 VortexDL。
// MV3 要求：事件监听必须在 Service Worker 顶层同步注册（不能放在异步回调里，
// 否则 SW 休眠唤醒时会漏掉事件）。
const DEFAULTS = { port: 24556, token: '' };

chrome.downloads.onCreated.addListener((item) => {
  const u = item.finalUrl || item.url || '';
  // data: 是超链接内联数据，无边界的文件语义，保持浏览器下载
  if (u.startsWith('data:')) return;
  // 先暂停再转发：小文件可能在转发完成前下完，导致取消竞态失败、两边各存一份
  chrome.downloads.pause(item.id).catch(() => {});
  if (u.startsWith('blob:')) {
    interceptBlob(item);
  } else {
    interceptUrl(item);
  }
});

// 普通网络下载：直接转发 URL，由桌面端多线程下载
async function interceptUrl(item) {
  try {
    const { port, token } = await chrome.storage.local.get(DEFAULTS);
    const ok = await forwardUrl(item.finalUrl || item.url, item.filename, port, token);
    await finalize(item, ok);
  } catch {
    await finalize(item, false);
  }
}

// 页面内存下载：内容脚本读取 blob 字节回传，再整体上传给桌面端落盘
async function interceptBlob(item) {
  if (item.tabId === undefined || item.tabId < 0) return finalize(item, false);
  try {
    const grabbed = await grabBlobFromTab(item);
    const { port, token } = await chrome.storage.local.get(DEFAULTS);
    const name = grabbed.name || baseName(item.filename) || 'download.bin';
    const ok = await forwardBlob(grabbed.parts, name, port, token);
    await finalize(item, ok);
  } catch {
    await finalize(item, false);
  }
}

async function finalize(item, ok) {
  flash(ok ? '✓' : '!', ok ? '#16a34a' : '#dc2626');
  if (!ok) {
    await chrome.downloads.resume(item.id).catch(() => {});
    return;
  }
  // 转交成功：清掉浏览器侧副本与下载列表记录。
  // blob 下载在内存中瞬间完成，cancel 往往来不及，必须再删文件
  await chrome.downloads.cancel(item.id).catch(() => {});
  await chrome.downloads.removeFile(item.id).catch(() => {});
  await chrome.downloads.erase({ id: item.id }).catch(() => {});
}

async function forwardUrl(url, filename, port, token) {
  const r = await fetch(`http://127.0.0.1:${port}/api/download`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', 'X-Token': token },
    body: JSON.stringify({ url, filename }),
  });
  const data = await r.json().catch(() => ({}));
  return !!(r.ok && data.ok);
}

async function forwardBlob(parts, name, port, token) {
  const body = new Blob(parts.map(base64ToBytes));
  const r = await fetch(
    `http://127.0.0.1:${port}/api/upload?name=${encodeURIComponent(name)}`,
    { method: 'POST', headers: { 'X-Token': token }, body }
  );
  const data = await r.json().catch(() => ({}));
  return !!(r.ok && data.ok);
}

// ── 内容脚本握手：请求读取 blob，分块回传后组装 ──

const pendingBlobs = new Map();

function grabBlobFromTab(item) {
  return new Promise((resolve, reject) => {
    const token = crypto.randomUUID();
    const timer = setTimeout(() => {
      pendingBlobs.delete(token);
      reject(new Error('读取 blob 超时'));
    }, 120000);
    pendingBlobs.set(token, { parts: [], timer, resolve, reject });
    chrome.tabs
      .sendMessage(item.tabId, { type: 'vortexdl-grab-blob', token, url: item.url })
      .catch((e) => {
        clearTimeout(timer);
        pendingBlobs.delete(token);
        reject(e);
      });
  });
}

chrome.runtime.onMessage.addListener((msg) => {
  if (!msg || typeof msg.token !== 'string') return;
  const st = pendingBlobs.get(msg.token);
  if (!st) return;
  if (msg.type === 'vortexdl-blob-part') {
    st.parts[msg.index] = msg.data;
  } else if (msg.type === 'vortexdl-blob-done') {
    clearTimeout(st.timer);
    pendingBlobs.delete(msg.token);
    st.resolve({ parts: st.parts, name: msg.name || '' });
  } else if (msg.type === 'vortexdl-blob-error') {
    clearTimeout(st.timer);
    pendingBlobs.delete(msg.token);
    st.reject(new Error(msg.error || '读取 blob 失败'));
  }
});

function base64ToBytes(b64) {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

function baseName(p) {
  if (!p) return '';
  const s = p.replace(/\\/g, '/');
  return s.slice(s.lastIndexOf('/') + 1);
}

function flash(text, color) {
  chrome.action.setBadgeText({ text });
  chrome.action.setBadgeBackgroundColor({ color });
}