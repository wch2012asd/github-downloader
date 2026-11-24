// 后端 Rust TaskRecord / IPC 事件的形状（与 src-tauri 对齐）
export interface BackendTask {
  id: string;
  kind: 'http' | 'bt';
  name: string;
  save_dir: string;
  category: string;
  status: 'queued' | 'downloading' | 'paused' | 'completed' | 'failed' | 'cancelled';
  total_bytes: number | null;
  received_bytes: number;
  created_at: number;
  finished_at: number | null;
  error: string | null;
  segments: number | null;
  limit_bps: number | null;
  use_clash: boolean;
  /** 首源 URL（后端 list_tasks 子查询附带） */
  url: string;
}
