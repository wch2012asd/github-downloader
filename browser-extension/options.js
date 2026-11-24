const DEFAULTS = { port: 24556, token: '' };

function readInputs() {
  return {
    port: Number(document.getElementById('port').value) || 24556,
    token: document.getElementById('token').value.trim(),
  };
}

function persist() {
  const cfg = readInputs();
  chrome.storage.local.set(cfg);
  return cfg;
}

chrome.storage.local.get(DEFAULTS).then(({ port, token }) => {
  document.getElementById('port').value = port;
  document.getElementById('token').value = token;
});

document.getElementById('save').addEventListener('click', async () => {
  persist();
  document.getElementById('save').textContent = '已保存';
  setTimeout(() => (document.getElementById('save').textContent = '保存'), 1200);
});

document.getElementById('test').addEventListener('click', async () => {
  const { port, token } = persist(); // 先保存再测，消除"测试绿但未保存"的坑
  const out = document.getElementById('test-result');
  out.textContent = '测试中…';
  out.style.color = '#6b7280';
  try {
    const r = await fetch(`http://127.0.0.1:${port}/api/download`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Token': token },
      body: JSON.stringify({ url: '' }),
    });
    const data = await r.json().catch(() => ({}));
    if (r.status === 403) {
      out.textContent = '✗ Token 不符（已自动保存当前输入，请确认与 VortexDL 设置一致）';
      out.style.color = '#dc2626';
    } else if (data.ok === false && data.error) {
      out.textContent = '✓ 连接成功（测试未带 url：' + data.error + '）';
      out.style.color = '#16a34a';
    } else {
      out.textContent = 'HTTP ' + r.status;
      out.style.color = '#dc2626';
    }
  } catch (e) {
    out.textContent = '✗ 无法连接 127.0.0.1:' + port + '（VortexDL 未运行？站点访问开关未开？）';
    out.style.color = '#dc2626';
  }
});
