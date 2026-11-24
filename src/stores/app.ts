// 应用状态中枢：任务/设置/日志/筛选/聚合统计。
// 双模式：Tauri 真实 IPC（list_tasks/Commands/事件流）与纯浏览器 dev（mock 种子 + 模拟节拍）。
import { defineStore } from 'pinia';
import { isTauri, api } from '../lib/bridge';
import type { BackendEvent } from '../lib/bridge';
import type { BackendTask } from '../lib/backend-types';
import { MOCK_TASKS } from '../data/mockTasks';
import { getFileCategory } from '../utils/format';
import type {
  AppSettings,
  DownloadTask,
  EngineLog,
  NewTaskPayload,
  TaskCategory,
  TaskSegment,
  ToastMessage,
  ViewFilter,
} from '../types';

const DEFAULT_SETTINGS: AppSettings = {
  maxConcurrent: 3,
  defaultSavePath: 'D:\\Downloads',
  defaultThreads: 8,
  globalDownloadLimit: 0,
  globalUploadLimit: 0,
  soundEnabled: true,
  theme: 'dark',
  dhtEnabled: true,
  btPort: 6881,
  proxyPort: 7890,
  browserTakeoverEnabled: true,
  browserTakeoverPort: 24556,
  browserTakeoverToken: '',
  turtleMode: false,
};

const STATUS_MAP: Record<BackendTask['status'], DownloadTask['status']> = {
  queued: 'waiting',
  downloading: 'downloading',
  paused: 'paused',
  completed: 'completed',
  failed: 'error',
  cancelled: 'cancelled',
};

const CATEGORY_MAP: Record<string, TaskCategory> = {
  archive: 'archive',
  app: 'software',
  video: 'video',
  doc: 'document',
};

/** 已完成/在途分段按顺序填充（等分段语义下与真实分段进度视觉一致；真实分段快照走 IPC 后替换） */
function deriveSegments(total: number, downloaded: number, count: number): TaskSegment[] {
  if (total <= 0) return []; // 总大小未知：不定态进度，由 UI 处理
  const c = Math.max(1, count);
  const chunk = Math.floor(total / c);
  const segs: TaskSegment[] = [];
  for (let i = 0; i < c; i++) {
    const startByte = i * chunk;
    const endByte = i === c - 1 ? total : (i + 1) * chunk;
    const segTotal = endByte - startByte;
    let segDownloaded = 0;
    if (downloaded >= endByte) segDownloaded = segTotal;
    else if (downloaded > startByte) segDownloaded = downloaded - startByte;
    const isComplete = segDownloaded >= segTotal;
    const active = !isComplete && segDownloaded > 0;
    segs.push({
      id: i + 1,
      startByte,
      endByte,
      downloadedBytes: segDownloaded,
      speed: 0,
      latencyMs: 18,
      status: isComplete ? 'completed' : active ? 'active' : 'idle',
    });
  }
  return segs;
}

function backendToTask(bt: BackendTask): DownloadTask {
  const total = bt.total_bytes ?? 0;
  const threads = bt.segments ?? 8;
  return {
    id: bt.id,
    name: bt.name,
    url: bt.url ?? '',
    category: CATEGORY_MAP[bt.category] ?? 'document',
    status: STATUS_MAP[bt.status] ?? 'waiting',
    totalSize: total,
    downloadedSize: bt.received_bytes,
    speed: 0,
    threads,
    segments: deriveSegments(total, bt.received_bytes, threads),
    createdAt: bt.created_at,
    completedAt: bt.finished_at ?? undefined,
    savePath: bt.save_dir,
    isTorrent: bt.kind === 'bt',
    errorMessage: bt.error ?? undefined,
    supportsRange: true,
    priority: 'normal',
    kind: bt.kind,
    useClash: bt.use_clash,
    limitBps: bt.limit_bps ?? undefined,
  };
}

export const useAppStore = defineStore('app', {
  state: () => ({
    tasks: [] as DownloadTask[],
    settings: { ...DEFAULT_SETTINGS },
    logs: [] as EngineLog[],
    speedHistory: Array(40).fill(0) as number[],
    searchQuery: '',
    viewFilter: 'all' as ViewFilter,
    categoryFilter: 'all' as TaskCategory | 'all',
    toasts: [] as ToastMessage[],
    initialized: false,
    dhtNodes: null as number | null,
    /** Clash 链路状态（30s 巡检；浏览器 dev 模式为演示值） */
    clash: {
      checking: false,
      ok: false,
      latencyMs: null as number | null,
    },
  }),

  getters: {
    turtleMode: (s) => s.settings.turtleMode,
    filteredTasks(s): DownloadTask[] {
      return s.tasks
        .filter((t) => {
          if (this.viewFilter === 'downloading' && t.status !== 'downloading') return false;
          if (this.viewFilter === 'completed' && t.status !== 'completed') return false;
          if (this.viewFilter === 'paused' && t.status !== 'paused' && t.status !== 'error') return false;
          if (this.categoryFilter !== 'all' && t.category !== this.categoryFilter) return false;
          const q = this.searchQuery.toLowerCase().trim();
          if (q) {
            const hit =
              t.name.toLowerCase().includes(q) ||
              t.url.toLowerCase().includes(q) ||
              t.category.toLowerCase().includes(q);
            if (!hit) return false;
          }
          return true;
        })
        .sort((a, b) => b.createdAt - a.createdAt);
    },
    aggregateStats(s) {
      let totalSpeed = 0;
      let totalUploadSpeed = 0;
      let activeCount = 0;
      let completedCount = 0;
      let pausedCount = 0;
      for (const t of s.tasks) {
        if (t.status === 'downloading') {
          totalSpeed += t.speed;
          totalUploadSpeed += t.uploadSpeed ?? 0;
          activeCount++;
        } else if (t.status === 'completed') completedCount++;
        else if (t.status === 'paused') pausedCount++;
      }
      return { totalSpeed, totalUploadSpeed, activeCount, completedCount, pausedCount, totalCount: s.tasks.length };
    },
  },

  actions: {
    async init() {
      if (this.initialized) return;
      this.initialized = true;
      this.addLog('极涌 VortexDL 多线程下载引擎内核初始化完成', 'info');
      if (isTauri) {
        await this.loadFromBackend();
        await this.loadSettingsFromBackend();
        await api.subscribeEvents((tid, ev: BackendEvent) => this.onBackendEvent(tid, ev));
        this.addLog('已连接本地下载内核（Tauri IPC）', 'info');
        this.startClashWatcher();
      } else {
        // 浏览器 dev 模式：演示数据 + 模拟节拍
        this.tasks = MOCK_TASKS.map((t) => ({ ...t }));
        this.clash = { checking: false, ok: true, latencyMs: 18 };
        this.addLog('DHT 路由表加载就绪: 488 个节点已加入分布式网络', 'info');
        this.addLog('加载现有下载任务队列 (共 8 个任务)', 'info');
        this.startSimulation();
      }
      this.startSpeedTicker();
      this.applyTheme();
    },

    /** Clash 链路巡检：立即一次 + 每 30s 复检（对齐 v2 周期） */
    async startClashWatcher() {
      const check = async () => {
        this.clash.checking = true;
        try {
          const r = await api.testClashProxy(this.settings.proxyPort);
          this.clash = {
            checking: false,
            ok: r.ok,
            latencyMs: r.latency_ms,
          };
        } catch {
          this.clash = { checking: false, ok: false, latencyMs: null };
        }
      };
      await check();
      setInterval(check, 30000);
    },

    async loadFromBackend() {
      const list = await api.listTasks();
      this.tasks = list.map(backendToTask);
    },

    async loadSettingsFromBackend() {
      try {
        const raw = await api.getSettings();
        const num = (k: string, d: number) => {
          const v = raw[k] !== undefined ? parseFloat(raw[k]) : NaN;
          return Number.isFinite(v) ? v : d;
        };
        this.settings = {
          ...this.settings,
          // 空字符串（首启默认库）回退到默认下载目录，避免建任务写目录失败
          defaultSavePath: raw.save_dir || this.settings.defaultSavePath,
          defaultThreads: num('segments', this.settings.defaultThreads),
          globalDownloadLimit: num('global_limit_bps', this.settings.globalDownloadLimit * 1024) / 1024,
          dhtEnabled: (raw.bt_enable_dht ?? 'true') === 'true',
          btPort: num('bt_listen_port', this.settings.btPort),
          proxyPort: num('proxy_port', this.settings.proxyPort),
          browserTakeoverEnabled:
            (raw.browser_takeover_enabled ?? 'true') === 'true',
          browserTakeoverPort: num('browser_takeover_port', this.settings.browserTakeoverPort),
          browserTakeoverToken: raw.browser_takeover_token ?? '',
          theme: (raw.theme_dark ?? 'true') === 'false' ? 'light' : 'dark',
        };
        this.settings.turtleMode = this.settings.globalDownloadLimit === 1024;
      } catch {
        // 设置读取失败保持默认
      }
    },

    /** 浏览器 dev 模式：800ms 模拟节拍（对齐设计稿模拟引擎） */
    startSimulation() {
      setInterval(() => {
        let totalCurrentSpeed = 0;
        for (const t of this.tasks) {
          if (t.status !== 'downloading') continue;
          const maxSpeed = this.settings.turtleMode ? 1024 * 1024 : 35 * 1024 * 1024;
          const variance = (Math.random() - 0.48) * 0.2;
          const base = t.speed > 0 ? t.speed : 15 * 1024 * 1024;
          const newSpeed = Math.max(500000, Math.min(maxSpeed, Math.floor(base * (1 + variance))));
          totalCurrentSpeed += newSpeed;
          const delta = Math.floor(newSpeed * 0.8);
          const newDownloaded = Math.min(t.totalSize, t.downloadedSize + delta);
          t.speed = newSpeed;
          t.downloadedSize = newDownloaded;
          // 分段顺序填充
          const remaining = t.segments.filter((s) => s.downloadedBytes < s.endByte - s.startByte);
          const segDelta = Math.floor(delta / (remaining.length || 1));
          for (const seg of t.segments) {
            const segTotal = seg.endByte - seg.startByte;
            if (seg.downloadedBytes >= segTotal) {
              seg.status = 'completed';
              seg.speed = 0;
              continue;
            }
            seg.downloadedBytes = Math.min(segTotal, seg.downloadedBytes + segDelta);
            const isDone = seg.downloadedBytes >= segTotal;
            seg.status = isDone ? 'completed' : 'active';
            seg.speed = isDone ? 0 : Math.floor(newSpeed / (t.segments.length || 1));
          }
          if (newDownloaded >= t.totalSize) {
            t.status = 'completed';
            t.speed = 0;
            t.downloadedSize = t.totalSize;
            t.completedAt = Date.now();
            this.addLog(`任务 [${t.name}] 下载完成！校验和已自动校验成功。`, 'info', t.id);
          }
        }
        this.speedHistory = [...this.speedHistory.slice(1), totalCurrentSpeed];
      }, 800);
    },

    onBackendEvent(tid: string, ev: BackendEvent) {
      if (ev.Log) {
        const [lv, msg] = ev.Log;
        const level = lv === 'Warn' ? 'warn' : lv === 'Error' ? 'error' : 'info';
        this.addLog(msg, level, tid);
        return;
      }
      const t = this.tasks.find((x) => x.id === tid);
      if (ev.Progress) {
        if (t) {
          t.downloadedSize = ev.Progress.downloaded;
          t.speed = ev.Progress.speed_bps;
          if (ev.Progress.total > 0) t.totalSize = ev.Progress.total;
          // 状态广播漏网兜底：有进度事件流动 = 引擎正在传输
          if (t.status !== 'completed' && t.status !== 'paused') t.status = 'downloading';
          // 分段状态跟随字节进度重排（等段顺序填充；真实分段快照经 IPC 后替换）
          t.segments = deriveSegments(
            t.totalSize,
            t.downloadedSize,
            t.segments.length || t.threads,
          );
        }
        return;
      }
      if (ev.BtProgress) {
        if (t) {
          t.downloadedSize = ev.BtProgress.downloaded;
          t.totalSize = ev.BtProgress.total;
          t.speed = ev.BtProgress.down_bps;
          t.uploadSpeed = ev.BtProgress.up_bps;
          if (t.status !== 'completed' && t.status !== 'paused') t.status = 'downloading';
        }
        if (ev.BtProgress.dht_nodes > 0) this.dhtNodes = ev.BtProgress.dht_nodes;
        return;
      }
      if (ev.Status) {
        // 管理器状态迁移广播：waiting/downloading/paused/completed/error/cancelled
        if (t) {
          const map: Record<string, DownloadTask['status']> = {
            queued: 'waiting',
            downloading: 'downloading',
            paused: 'paused',
            completed: 'completed',
            failed: 'error',
            cancelled: 'cancelled',
          };
          t.status = map[ev.Status] ?? t.status;
        }
        return;
      }
      if (ev.Finished) {
        // 引擎收尾：以最新后端状态为准刷新该任务
        api
          .listTasks()
          .then((list) => {
            const bt = list.find((x) => x.id === tid);
            if (!bt) return;
            const idx = this.tasks.findIndex((x) => x.id === tid);
            if (idx >= 0) this.tasks[idx] = backendToTask(bt);
          })
          .catch(() => {});
        if (!ev.Finished.ok && ev.Finished.reason) {
          this.addToast(`任务失败: ${ev.Finished.reason}`, 'error');
        }
      }
    },

    /** 波形节拍：固定 500ms 采样聚合速度——空闲时平线在 0 基线，波形永远"活" */
    startSpeedTicker() {
      setInterval(() => {
        this.speedHistory = [
          ...this.speedHistory.slice(1),
          this.aggregateStats.totalSpeed,
        ];
      }, 500);
    },

    applyTheme() {
      if (this.settings.theme === 'dark') document.documentElement.classList.add('dark');
      else document.documentElement.classList.remove('dark');
    },

    addLog(message: string, level: EngineLog['level'] = 'info', taskId?: string) {
      this.logs.unshift({
        id: Math.random().toString(36).substring(2, 9),
        timestamp: new Date().toLocaleTimeString('zh-CN', { hour12: false }),
        level,
        message,
        taskId,
      });
      if (this.logs.length > 200) this.logs.length = 200;
    },

    addToast(text: string, type: ToastMessage['type'] = 'success') {
      const id = Math.random().toString(36).substring(2, 9);
      this.toasts.push({ id, type, text });
      setTimeout(() => this.dismissToast(id), 2800);
    },

    dismissToast(id: string) {
      this.toasts = this.toasts.filter((t) => t.id !== id);
    },

    async toggleTaskStatus(id: string) {
      const t = this.tasks.find((x) => x.id === id);
      if (!t) return;
      if (t.status === 'downloading') {
        t.status = 'paused';
        t.speed = 0;
        if (isTauri) await api.pause(id);
        this.addLog('任务已暂停', 'info', id);
      } else if (t.status === 'paused' || t.status === 'error' || t.status === 'waiting') {
        t.status = 'downloading';
        t.errorMessage = undefined;
        if (isTauri) await api.resume(id);
        this.addLog('任务已恢复连接并开始分配多线程分块', 'info', id);
      }
    },

    async deleteTask(id: string, deleteFiles = false) {
      if (isTauri) await api.remove(id, deleteFiles);
      else if (deleteFiles) {
        // 浏览器演示模式不落盘，仅移除记录
      }
      this.tasks = this.tasks.filter((t) => t.id !== id);
      this.addLog(deleteFiles ? '任务与源文件已删除' : '任务已被移除', 'warn', id);
    },

    async openFile(id: string) {
      if (!isTauri) {
        this.addToast('浏览器演示模式不支持打开文件', 'info');
        return;
      }
      try {
        await api.openFile(id);
      } catch (e) {
        this.addToast(`${String(e)}，已改为打开所在文件夹`, 'info');
        await this.openFolder(id);
      }
    },

    async openFolder(id: string) {
      if (!isTauri) {
        this.addToast('浏览器演示模式不支持打开文件夹', 'info');
        return;
      }
      try {
        await api.openDir(id);
      } catch (e) {
        this.addToast(`打开文件夹失败: ${String(e)}`, 'error');
      }
    },

    /** 对已有任务切换 Clash 通道：取消当前传输 → 翻转 use_clash → 同源重建（断点续传保留） */
    async toggleTaskClash(task: DownloadTask) {
      if (!isTauri) {
        this.addToast('浏览器演示模式不支持切换通道', 'info');
        return;
      }
      const next = !task.useClash;
      await api.cancel(task.id);
      // 等引擎退出（最多 10s）
      for (let i = 0; i < 100; i++) {
        const list = await api.listTasks();
        const t = list.find((x) => x.id === task.id);
        if (!t || t.status !== 'downloading') break;
        await new Promise((r) => setTimeout(r, 100));
      }
      await api.remove(task.id, false);
      const newId = await api.createHttpTask({
        urls: [task.url],
        name: task.name,
        saveDir: task.savePath,
        segments: task.threads,
        limitBps: task.limitBps ?? undefined,
        useClash: next,
      });
      const list = await api.listTasks();
      const idx = this.tasks.findIndex((x) => x.id === task.id);
      const bt = list.find((x) => x.id === newId);
      if (bt && idx >= 0) this.tasks[idx] = backendToTask(bt);
      this.addLog(
        next ? '已切换为 Clash 代理通道，断点续传中' : '已切换为直连通道，断点续传中',
        'info',
        newId,
      );
    },

    async restartTask(id: string) {
      // 重新下载：后端清断点从零开始（HTTP）/ 重新校验（BT），任务 id 不变
      if (isTauri) {
        try {
          await api.restartTask(id);
          await this.loadFromBackend();
          this.addLog('任务已重新开始下载', 'info', id);
          this.addToast('已重新开始下载', 'info');
        } catch (e) {
          this.addToast(`重新下载失败: ${String(e)}`, 'error');
        }
      } else {
        const t = this.tasks.find((x) => x.id === id);
        if (t) {
          t.status = 'downloading';
          t.downloadedSize = 0;
          t.speed = 15000000;
          t.errorMessage = undefined;
        }
        this.addLog('重新发起下载任务', 'info', id);
      }
    },

    async pauseAll() {
      for (const t of this.tasks.filter((x) => x.status === 'downloading')) {
        t.status = 'paused';
        t.speed = 0;
        if (isTauri) await api.pause(t.id);
      }
      this.addLog('全局操作：已暂停全部进行中的任务', 'info');
    },

    async resumeAll() {
      for (const t of this.tasks.filter((x) => x.status === 'paused')) {
        t.status = 'downloading';
        if (isTauri) await api.resume(t.id);
      }
      this.addLog('全局操作：已恢复队列中所有暂停的任务', 'info');
    },

    async clearCompleted() {
      const completed = this.tasks.filter((t) => t.status === 'completed');
      for (const t of completed) {
        if (isTauri) await api.remove(t.id);
      }
      this.tasks = this.tasks.filter((t) => t.status !== 'completed');
      this.addLog('已清除所有已完成任务记录', 'info');
    },

    async addTask(payload: NewTaskPayload) {
      if (isTauri) {
        let newId: string | undefined;
        if (payload.isTorrent) {
          newId = await api.createBtTask({ source: payload.url, saveDir: payload.savePath ?? this.settings.defaultSavePath });
        } else {
          newId = await api.createHttpTask({
            urls: [payload.url],
            name: payload.name,
            saveDir: payload.savePath ?? this.settings.defaultSavePath,
            segments: payload.threads ?? this.settings.defaultThreads,
            limitBps: payload.limitBps,
            useClash: payload.useClash ?? false,
          });
        }
        // autoStart=false：创建后立即暂停（引擎默认入队即启动）
        if (payload.autoStart === false && newId) await api.pause(newId);
        await this.loadFromBackend();
      } else {
        const size = payload.totalSize ?? Math.floor(Math.random() * 2000000000 + 100000000);
        const threads = payload.threads ?? this.settings.defaultThreads;
        const category = payload.category ?? getFileCategory(payload.name);
        const segs: TaskSegment[] = [];
        const chunk = Math.floor(size / threads);
        for (let i = 0; i < threads; i++) {
          segs.push({
            id: i + 1,
            startByte: i * chunk,
            endByte: i === threads - 1 ? size : (i + 1) * chunk,
            downloadedBytes: 0,
            speed: 0,
            latencyMs: 18,
            status: 'idle',
          });
        }
        this.tasks.unshift({
          id: `task-${Date.now()}`,
          name: payload.name,
          url: payload.url,
          category,
          status: payload.autoStart !== false ? 'downloading' : 'paused',
          totalSize: size,
          downloadedSize: 0,
          speed: payload.autoStart !== false ? 12000000 : 0,
          threads,
          segments: segs,
          createdAt: Date.now(),
          savePath: payload.savePath ?? this.settings.defaultSavePath,
          isTorrent: payload.isTorrent,
          priority: 'normal',
          kind: payload.isTorrent ? 'bt' : 'http',
          useClash: payload.useClash ?? false,
        });
      }
      this.addLog(`成功创建新下载任务: ${payload.name}`, 'info');
    },

    async updateSettings(partial: Partial<AppSettings>) {
      const prev = { ...this.settings };
      this.settings = { ...prev, ...partial };
      this.applyTheme();
      if (!isTauri) return;
      try {
        if (partial.defaultSavePath !== undefined) await api.setSetting('save_dir', partial.defaultSavePath);
        if (partial.defaultThreads !== undefined) await api.setSetting('segments', String(partial.defaultThreads));
        if (partial.globalDownloadLimit !== undefined)
          await api.setSetting('global_limit_bps', String(Math.round(partial.globalDownloadLimit * 1024)));
        if (partial.theme !== undefined) await api.setSetting('theme_dark', String(partial.theme === 'dark'));
        if (partial.dhtEnabled !== undefined) await api.setSetting('bt_enable_dht', String(partial.dhtEnabled));
        if (partial.btPort !== undefined) await api.setSetting('bt_listen_port', String(partial.btPort));
        if (partial.proxyPort !== undefined) await api.setSetting('proxy_port', String(partial.proxyPort));
        if (partial.browserTakeoverEnabled !== undefined)
          await api.setSetting('browser_takeover_enabled', String(partial.browserTakeoverEnabled));
        if (partial.browserTakeoverPort !== undefined)
          await api.setSetting('browser_takeover_port', String(partial.browserTakeoverPort));
        if (partial.browserTakeoverToken !== undefined)
          await api.setSetting('browser_takeover_token', partial.browserTakeoverToken);
      } catch (e) {
        this.addToast(`设置保存失败: ${String(e)}`, 'error');
      }
    },

    async toggleTurtleMode() {
      const next = !this.settings.turtleMode;
      this.settings.turtleMode = next;
      this.addLog(next ? '乌龟限速模式已开启 (全局限速 1 MB/s)' : '乌龟限速模式已关闭 (全速下载)', 'warn');
      if (isTauri) await api.setSetting('global_limit_bps', next ? '1048576' : '0');
    },
  },
});
