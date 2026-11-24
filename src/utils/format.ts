// 格式化工具：对齐设计稿 utils/format.ts

export function formatBytes(bytes: number, decimals = 1): string {
  // <1 视为 0（同时挡住浮点下溢产生的科学计数值）
  if (!bytes || bytes < 1) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  const sizeIdx = Math.min(Math.max(i, 0), sizes.length - 1);
  return `${parseFloat((bytes / Math.pow(k, sizeIdx)).toFixed(decimals))} ${sizes[sizeIdx]}`;
}

export function formatSpeed(bytesPerSec: number): string {
  if (!bytesPerSec || bytesPerSec < 1) return '0 KB/s';
  return `${formatBytes(bytesPerSec)}/s`;
}

export function formatEta(remainingBytes: number, speedBytesPerSec: number): string {
  if (!speedBytesPerSec || speedBytesPerSec < 1) return '--:--';
  const seconds = Math.floor(remainingBytes / speedBytesPerSec);
  if (seconds < 0) return '00:00';
  if (seconds > 86400 * 7) return '> 7 天';
  const hrs = Math.floor(seconds / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  const secs = seconds % 60;
  if (hrs > 0) return `${hrs}h ${mins.toString().padStart(2, '0')}m`;
  return `${mins.toString().padStart(2, '0')}m ${secs.toString().padStart(2, '0')}s`;
}

export function formatDate(timestamp: number): string {
  const date = new Date(timestamp);
  const now = new Date();
  const isToday = date.toDateString() === now.toDateString();
  const timeStr = date.toLocaleTimeString('zh-CN', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  });
  if (isToday) return `今天 ${timeStr}`;
  const month = (date.getMonth() + 1).toString().padStart(2, '0');
  const day = date.getDate().toString().padStart(2, '0');
  return `${month}-${day} ${timeStr}`;
}

export function getFileCategory(filename: string): 'video' | 'archive' | 'software' | 'document' | 'torrent' {
  const ext = filename.split('.').pop()?.toLowerCase() || '';
  if (['mp4', 'mkv', 'avi', 'mov', 'flv', 'wmv', 'webm', 'ts'].includes(ext)) return 'video';
  if (['zip', 'rar', '7z', 'tar', 'gz', 'iso', 'xz', 'bz2', 'dmg'].includes(ext)) return 'archive';
  if (['exe', 'msi', 'apk', 'pkg', 'deb', 'appimage', 'bin'].includes(ext)) return 'software';
  if (ext === 'torrent') return 'torrent';
  return 'document';
}
