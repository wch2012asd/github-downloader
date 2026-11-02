export type GitHubResourceType = 
  | 'release-asset'
  | 'source-archive'
  | 'raw-file'
  | 'git-repo'
  | 'general-url';

export interface ParsedGitHubInfo {
  isValid: boolean;
  type: GitHubResourceType;
  typeLabel: string;
  owner?: string;
  repo?: string;
  tagOrBranch?: string;
  filename: string;
  originalUrl: string;
  estimatedSize?: string;
  rawSizeNum?: number;
  assets?: Array<{
    name: string;
    size: string;
    downloadUrl: string;
  }>;
}

export type AccelerationEngine = 
  | 'clash'       // 本地 Clash / 系统代理
  | 'ghproxy'     // GHProxy 镜像节点
  | 'kkgithub'    // KKGitHub 加速
  | 'moeyy'       // Moeyy 节点
  | 'direct';     // 官方直连 (无加速)

export interface AccelerationOption {
  id: AccelerationEngine;
  name: string;
  description: string;
  badge: string;
  latencyHint: string;
  pingState?: 'fast' | 'medium' | 'slow';
}

export interface ProxySettings {
  port: number;
  protocol: 'HTTP' | 'SOCKS5';
  status: 'idle' | 'checking' | 'connected' | 'error';
  latencyMs: number | null;
  message?: string;
}

export type DownloadStatus = 
  | 'idle'
  | 'parsing'
  | 'downloading'
  | 'paused'
  | 'completed'
  | 'cancelled'
  | 'error';

export interface DownloadTask {
  id: string;
  url: string;
  acceleratedUrl: string;
  filename: string;
  savePath: string;
  status: DownloadStatus;
  progress: number;            // 0 - 100
  speedBps: number;            // bytes/s
  bytesDownloaded: number;
  totalBytes: number;
  etaSeconds: number;
  threads: number;
  createdAt: number;
  completedAt?: number;
}

export type LogLevel = 'INFO' | 'SUCCESS' | 'WARN' | 'ERROR' | 'PROXY';

export interface LogEntry {
  id: string;
  timestamp: string;
  level: LogLevel;
  message: string;
}
