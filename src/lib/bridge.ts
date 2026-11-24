// Tauri IPC 桥接层：Tauri 真实模式走 Commands/Channel；纯浏览器 dev 模式优雅回退（由 store 决定）
import type { BackendTask } from './backend-types';

export const isTauri = '__TAURI_INTERNALS__' in window;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(cmd, args);
}

export interface BackendEvent {
  Log?: [string, string]; // [Level, message]
  Progress?: { downloaded: number; total: number; speed_bps: number; pct: number };
  BtProgress?: {
    downloaded: number;
    total: number;
    down_bps: number;
    up_bps: number;
    pct: number;
    dht_nodes: number;
  };
  /** 后端状态串：queued/downloading/paused/completed/failed/cancelled */
  Status?: string;
  Finished?: { ok: boolean; reason: string };
}

export const api = {
  listTasks(): Promise<BackendTask[]> {
    return invoke('list_tasks');
  },
  createHttpTask(payload: {
    urls: string[];
    name: string;
    saveDir: string;
    segments?: number;
    limitBps?: number;
    useClash?: boolean;
  }): Promise<string> {
    // Tauri v2 命令参数要求 JS 侧驼峰命名（saveDir → Rust 的 save_dir）
    return invoke('create_http_task', payload);
  },
  createBtTask(payload: { source: string; saveDir: string }): Promise<string> {
    return invoke('create_bt_task', payload);
  },
  /** 编辑任务（HTTP）：名称/目录/线程数/Clash 通道；运行中会自动续传 */
  updateTask(payload: {
    id: string;
    name: string;
    saveDir: string;
    segments?: number;
    useClash: boolean;
  }): Promise<void> {
    return invoke('update_task', payload);
  },
  /** 重新下载：清断点从零开始（保留任务 id 与已完成的最终文件） */
  restartTask(id: string): Promise<void> {
    return invoke('restart_task', { id });
  },
  pause(id: string): Promise<void> {
    return invoke('pause_task', { id });
  },
  resume(id: string): Promise<void> {
    return invoke('resume_task', { id });
  },
  cancel(id: string): Promise<void> {
    return invoke('cancel_task', { id });
  },
  remove(id: string, deleteFiles?: boolean): Promise<void> {
    return invoke('remove_task', { id, deleteFiles: deleteFiles ?? false });
  },
  openDir(id: string): Promise<void> {
    return invoke('open_task_dir', { id });
  },
  openFile(id: string): Promise<void> {
    return invoke('open_task_file', { id });
  },
  async getSettings(): Promise<Record<string, string>> {
    return invoke('get_settings');
  },
  setSetting(key: string, value: string): Promise<void> {
    return invoke('set_setting', { key, value });
  },
  importV2Config(path: string): Promise<number> {
    return invoke('import_v2_config', { path });
  },
  /** 自动检测 Clash 端口（注册表 + 常见端口扫描）；未找到返回 null */
  detectClashPort(): Promise<number | null> {
    return invoke('detect_clash_port');
  },
  /** 实测 Clash 链路；tcp_open=true + ok=false = 端口通但经代理的请求失败（节点波动） */
  testClashProxy(port: number): Promise<{ ok: boolean; latency_ms: number | null; tcp_open: boolean }> {
    return invoke('test_clash_proxy', { port });
  },
  /** 订阅引擎事件流；返回取消订阅函数 */
  async subscribeEvents(
    handler: (taskId: string, ev: BackendEvent) => void,
  ): Promise<() => void> {
    const { Channel } = await import('@tauri-apps/api/core');
    const channel = new Channel<[string, BackendEvent]>();
    channel.onmessage = (msg) => handler(msg[0], msg[1]);
    await invoke('subscribe_events', { onEvent: channel });
    return () => {
      // Channel 无显式关闭 API；前端侧忽略后续消息即可
      channel.onmessage = () => {};
    };
  },
  getWindow() {
    if (!isTauri) return null;
    // 动态导入，浏览器模式下不触发
    return import('@tauri-apps/api/window').then((m) => m.getCurrentWindow());
  },
};
