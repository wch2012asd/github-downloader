// 在页面内读取 blob: 下载内容并分块回传给扩展后台。
// blob 数据只存在于页面内存，Service Worker 无法直接 fetch，必须由内容脚本代取。
const CHUNK = 3 * 1024 * 1024;

// ChatGPT 等页面点击下载链接时会带 download 属性给出真实文件名，点击瞬间记录
let lastBlobDownload = null;

document.addEventListener(
  'click',
  (e) => {
    const a = e.target instanceof Element ? e.target.closest('a[href^="blob:"]') : null;
    if (a) lastBlobDownload = { url: a.href, name: a.download || '' };
  },
  true
);

chrome.runtime.onMessage.addListener((msg) => {
  if (!msg || msg.type !== 'vortexdl-grab-blob') return;
  grab(msg.token, msg.url);
});

async function grab(token, url) {
  try {
    let name = '';
    if (lastBlobDownload && lastBlobDownload.url === url) name = lastBlobDownload.name;
    if (!name) {
      const a = document.querySelector(`a[href="${CSS.escape(url)}"]`);
      if (a && a.download) name = a.download;
    }
    const res = await fetch(url);
    if (!res.ok) throw new Error('HTTP ' + res.status);
    const buf = new Uint8Array(await res.arrayBuffer());
    let index = 0;
    for (let i = 0; i < buf.length; i += CHUNK, index++) {
      await chrome.runtime.sendMessage({
        type: 'vortexdl-blob-part',
        token,
        index,
        data: toBase64(buf.subarray(i, i + CHUNK)),
      });
    }
    await chrome.runtime.sendMessage({ type: 'vortexdl-blob-done', token, name });
  } catch (e) {
    chrome.runtime
      .sendMessage({
        type: 'vortexdl-blob-error',
        token,
        error: String((e && e.message) || e),
      })
      .catch(() => {});
  }
}

// 分块编码，避免 String.fromCharCode 参数过多导致栈溢出
function toBase64(bytes) {
  let s = '';
  for (let i = 0; i < bytes.length; i += 0x8000) {
    s += String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000));
  }
  return btoa(s);
}