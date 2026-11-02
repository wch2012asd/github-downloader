import { AccelerationEngine, GitHubResourceType, ParsedGitHubInfo } from '../types';

export function parseGitHubUrl(rawInput: string): ParsedGitHubInfo {
  const url = rawInput.trim();
  if (!url) {
    return {
      isValid: false,
      type: 'general-url',
      typeLabel: '未输入链接',
      filename: '',
      originalUrl: '',
    };
  }

  try {
    const parsed = new URL(url.startsWith('http') ? url : `https://${url}`);
    const host = parsed.hostname.toLowerCase();
    const pathname = parsed.pathname;

    // Check if it's raw.githubusercontent.com
    if (host.includes('raw.githubusercontent.com')) {
      const parts = pathname.split('/').filter(Boolean);
      const owner = parts[0] || '';
      const repo = parts[1] || '';
      const branchOrTag = parts[2] || 'master';
      const filename = parts[parts.length - 1] || 'file.txt';

      return {
        isValid: true,
        type: 'raw-file',
        typeLabel: 'Raw 原始文件',
        owner,
        repo,
        tagOrBranch: branchOrTag,
        filename,
        originalUrl: url,
        estimatedSize: '约 150 KB',
        rawSizeNum: 153600,
      };
    }

    // Check github.com
    if (host.includes('github.com')) {
      const parts = pathname.split('/').filter(Boolean);
      const owner = parts[0] || '';
      const repo = parts[1] || '';

      // Release asset: /:owner/:repo/releases/download/:tag/:file
      if (pathname.includes('/releases/download/')) {
        const afterDownload = pathname.split('/releases/download/')[1];
        const subParts = afterDownload.split('/');
        const tag = subParts[0] || 'latest';
        const filename = decodeURIComponent(subParts.slice(1).join('/') || 'asset.zip');

        // Estimate typical release file size
        const isExe = filename.endsWith('.exe') || filename.endsWith('.msi');
        const isDmg = filename.endsWith('.dmg');
        const isZip = filename.endsWith('.zip') || filename.endsWith('.tar.gz');
        let estimated = '约 45.2 MB';
        let rawNum = 45200000;
        if (isExe || isDmg) {
          estimated = '约 84.6 MB';
          rawNum = 88713000;
        } else if (isZip) {
          estimated = '约 32.8 MB';
          rawNum = 34400000;
        }

        return {
          isValid: true,
          type: 'release-asset',
          typeLabel: 'Release 资产包',
          owner,
          repo,
          tagOrBranch: tag,
          filename,
          originalUrl: url,
          estimatedSize: estimated,
          rawSizeNum: rawNum,
        };
      }

      // Archive / zip: /:owner/:repo/archive/...
      if (pathname.includes('/archive/')) {
        const filename = `${repo || 'source'}-${pathname.split('/').pop() || 'code.zip'}`;
        return {
          isValid: true,
          type: 'source-archive',
          typeLabel: '源码压缩包 (Zip)',
          owner,
          repo,
          filename: filename.endsWith('.zip') ? filename : `${filename}.zip`,
          originalUrl: url,
          estimatedSize: '约 18.5 MB',
          rawSizeNum: 19400000,
        };
      }

      // Raw blob view: /:owner/:repo/raw/...
      if (pathname.includes('/raw/')) {
        const parts = pathname.split('/raw/')[1].split('/');
        const filename = parts.pop() || 'file';
        return {
          isValid: true,
          type: 'raw-file',
          typeLabel: 'Raw 原始文件',
          owner,
          repo,
          filename,
          originalUrl: url,
          estimatedSize: '约 48 KB',
          rawSizeNum: 49152,
        };
      }

      // Releases page: /:owner/:repo/releases
      if (pathname.includes('/releases')) {
        return {
          isValid: true,
          type: 'release-asset',
          typeLabel: 'Release 资产集合',
          owner,
          repo,
          filename: `${repo}-latest-release.zip`,
          originalUrl: url,
          estimatedSize: '约 64.0 MB',
          rawSizeNum: 67108864,
          assets: [
            { name: `${repo}_Windows_x64.exe`, size: '78.4 MB', downloadUrl: `${url}/download/v1.0.0/${repo}_Windows_x64.exe` },
            { name: `${repo}_macOS_universal.dmg`, size: '82.1 MB', downloadUrl: `${url}/download/v1.0.0/${repo}_macOS_universal.dmg` },
            { name: `${repo}_Linux_amd64.tar.gz`, size: '65.3 MB', downloadUrl: `${url}/download/v1.0.0/${repo}_Linux_amd64.tar.gz` },
          ],
        };
      }

      // General Repo
      if (owner && repo) {
        return {
          isValid: true,
          type: 'git-repo',
          typeLabel: 'Git 仓库代码包',
          owner,
          repo,
          filename: `${repo}-main.zip`,
          originalUrl: url,
          estimatedSize: '约 24.3 MB',
          rawSizeNum: 25480000,
        };
      }
    }

    // Generic URL fallback
    const pathParts = pathname.split('/').filter(Boolean);
    const filename = pathParts.length ? pathParts[pathParts.length - 1] : 'download.bin';

    return {
      isValid: true,
      type: 'general-url',
      typeLabel: '外部下载链接',
      filename,
      originalUrl: url,
      estimatedSize: '未知大小',
      rawSizeNum: 10485760,
    };
  } catch {
    return {
      isValid: false,
      type: 'general-url',
      typeLabel: '无效 URL',
      filename: '',
      originalUrl: rawInput,
    };
  }
}

export function buildAcceleratedUrl(originalUrl: string, engine: AccelerationEngine): string {
  if (!originalUrl) return '';
  switch (engine) {
    case 'ghproxy':
      return `https://ghproxy.net/${originalUrl}`;
    case 'kkgithub':
      return originalUrl.replace('github.com', 'kkgithub.com');
    case 'moeyy':
      return `https://github.moeyy.xyz/${originalUrl}`;
    case 'clash':
      // Local clash proxy handles the standard github url through 127.0.0.1:port
      return originalUrl;
    case 'direct':
    default:
      return originalUrl;
  }
}

export function formatBytes(bytes: number, decimals = 1): string {
  if (bytes <= 0) return '0 B';
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(dm))} ${sizes[i]}`;
}

export function formatSpeed(bytesPerSec: number): string {
  if (bytesPerSec <= 0) return '--';
  if (bytesPerSec < 1024) return `${Math.round(bytesPerSec)} B/s`;
  if (bytesPerSec < 1024 * 1024) return `${(bytesPerSec / 1024).toFixed(1)} KB/s`;
  return `${(bytesPerSec / (1024 * 1024)).toFixed(1)} MB/s`;
}

export function formatEta(seconds: number): string {
  if (!seconds || seconds <= 0 || !isFinite(seconds)) return '--';
  if (seconds < 60) return `${Math.ceil(seconds)} 秒`;
  const mins = Math.floor(seconds / 60);
  const secs = Math.ceil(seconds % 60);
  if (mins < 60) return `${mins}分 ${secs}秒`;
  const hours = Math.floor(mins / 60);
  return `${hours}小时 ${mins % 60}分`;
}

export function getNowTimestamp(): string {
  const now = new Date();
  const pad = (n: number) => n.toString().padStart(2, '0');
  return `[${pad(now.getHours())}:${pad(now.getMinutes())}:${pad(now.getSeconds())}]`;
}

export const PRESET_DEMO_LINKS = [
  {
    name: 'LumaShot 安装包 (v0.3.0)',
    category: 'Release 资产',
    size: '65.8 MB',
    url: 'https://github.com/jimmgreen/LumaShot/releases/download/v0.3.0/LumaShot-Setup.exe',
  },
  {
    name: 'Clash Verge Rev 安装包 (exe)',
    category: 'Release 资产',
    size: '86.4 MB',
    url: 'https://github.com/clash-verge-rev/clash-verge-rev/releases/download/v1.7.7/Clash.Verge_1.7.7_x64-setup.exe',
  },
  {
    name: 'VS Code 用户安装包 (Windows)',
    category: 'Release 资产',
    size: '98.1 MB',
    url: 'https://github.com/microsoft/vscode/releases/download/1.96.2/VSCodeUserSetup-x64-1.96.2.exe',
  },
  {
    name: 'v2rayA 源码包 (v2.2.5.zip)',
    category: 'Source Archive',
    size: '14.2 MB',
    url: 'https://github.com/v2rayA/v2rayA/archive/refs/tags/v2.2.5.zip',
  },
  {
    name: 'Linux 内核 README 文档',
    category: 'Raw 单文件',
    size: '18.2 KB',
    url: 'https://raw.githubusercontent.com/torvalds/linux/master/README',
  },
];
