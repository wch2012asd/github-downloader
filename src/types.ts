// 前端数据模型：UI 词汇对齐设计稿（design-ref-v3/types/download.ts），
// 后端（Rust TaskRecord/IPC 事件）经 store 的映射函数转换为本类型。

export type TaskStatus =
  | 'downloading'
  | 'paused'
  | 'completed'
  | 'error'
  | 'waiting'
  | 'cancelled';

export type TaskCategory = 'video' | 'archive' | 'software' | 'document' | 'torrent';

export interface TaskSegment {
  id: number;
  startByte: number;
  endByte: number;
  downloadedBytes: number;
  speed: number; // bytes/s
  latencyMs: number;
  status: 'active' | 'completed' | 'idle' | 'stalled';
  workerIp?: string;
}

export interface PeerInfo {
  id: string;
  ip: string;
  client: string;
  downloadSpeed: number;
  uploadSpeed: number;
  progress: number;
  country: string;
}

export interface MirrorSource {
  id: string;
  name: string;
  url: string;
  latencyMs: number;
  speed: number;
  status: 'active' | 'standby' | 'error';
  isFastest?: boolean;
}

export interface DownloadTask {
  id: string;
  name: string;
  url: string;
  category: TaskCategory;
  status: TaskStatus;
  totalSize: number;
  downloadedSize: number;
  speed: number; // bytes/s
  uploadSpeed?: number;
  threads: number;
  segments: TaskSegment[];
  createdAt: number;
  completedAt?: number;
  savePath: string;
  isTorrent?: boolean;
  errorMessage?: string;
  supportsRange?: boolean;
  priority?: 'low' | 'normal' | 'high';
  bandwidthLimitKb?: number; // 0 = 不限
  limitBps?: number; // 任务级限速（B/s）
  kind: 'http' | 'bt';
  useClash?: boolean; // 经 Clash 本地代理下载（HTTP 任务）
}

export type ViewFilter = 'all' | 'downloading' | 'completed' | 'paused';

export interface AppSettings {
  maxConcurrent: number;
  defaultSavePath: string;
  defaultThreads: number;
  globalDownloadLimit: number; // KB/s，0 = 不限
  globalUploadLimit: number; // KB/s
  soundEnabled: boolean;
  theme: 'dark' | 'light';
  dhtEnabled: boolean;
  btPort: number;
  proxyPort: number; // Clash 本地代理端口
  browserTakeoverEnabled: boolean;
  browserTakeoverPort: number;
  browserTakeoverToken: string;
  turtleMode: boolean;
}

export interface EngineLog {
  id: string;
  timestamp: string;
  level: 'info' | 'warn' | 'error' | 'debug';
  message: string;
  taskId?: string;
}

export interface ToastMessage {
  id: string;
  type: 'success' | 'error' | 'info';
  text: string;
}

export interface NewTaskPayload {
  name: string;
  url: string;
  totalSize?: number;
  threads?: number;
  savePath?: string;
  category?: TaskCategory;
  isTorrent?: boolean;
  autoStart?: boolean;
  useClash?: boolean;
  limitBps?: number;
}
