<script setup lang="ts">
// 任务流列表：状态×格式筛选条 + 任务卡片（扩展名徽标/分段流管/行内操作/展开遥测架）
import { onBeforeUnmount, onMounted, ref } from 'vue';
import {
  Play, Pause, RotateCw, FolderOpen, Trash2, Copy, ChevronDown, ChevronUp,
  CheckCircle2, Check, Film, Archive, Package, FileText, Magnet, Archive as ArchiveIcon,
  ShieldCheck, Pencil,
} from 'lucide-vue-next';
import EditTaskModal from './EditTaskModal.vue';
import { useAppStore } from '../stores/app';
import { api, isTauri } from '../lib/bridge';
import { formatBytes, formatSpeed, formatEta, formatDate } from '../utils/format';
import type { DownloadTask, TaskCategory, ViewFilter } from '../types';

const store = useAppStore();
const expandedTaskId = ref<string | null>(null);
const copiedHash = ref<string | null>(null);
const ctxMenu = ref<{ x: number; y: number; task: DownloadTask } | null>(null);
const deleteConfirm = ref<{ task: DownloadTask; deleteFiles: boolean } | null>(null);
const editingTask = ref<DownloadTask | null>(null);

onMounted(() => document.addEventListener('click', closeCtx));
onBeforeUnmount(() => document.removeEventListener('click', closeCtx));

function openCtx(e: MouseEvent, task: DownloadTask) {
  // 边缘防溢出：菜单尺寸约 160x90
  const x = Math.min(e.clientX, window.innerWidth - 170);
  const y = Math.min(e.clientY, window.innerHeight - 100);
  ctxMenu.value = { x, y, task };
}
function closeCtx() {
  ctxMenu.value = null;
}
function ctxOpenFile(task: DownloadTask) {
  ctxMenu.value = null;
  store.openFile(task.id);
}
function ctxOpenFolder(task: DownloadTask) {
  ctxMenu.value = null;
  store.openFolder(task.id);
}
function ctxEdit(task: DownloadTask) {
  ctxMenu.value = null;
  editingTask.value = task;
}
function askDelete(task: DownloadTask) {
  deleteConfirm.value = { task, deleteFiles: false };
}
async function confirmDelete() {
  if (!deleteConfirm.value) return;
  const { task, deleteFiles } = deleteConfirm.value;
  deleteConfirm.value = null;
  await store.deleteTask(task.id, deleteFiles);
}

const VIEW_FILTERS: { id: ViewFilter; label: string }[] = [
  { id: 'all', label: '全部' },
  { id: 'downloading', label: '下载中' },
  { id: 'completed', label: '已完成' },
  { id: 'paused', label: '暂停/排队' },
];

const CATEGORY_FILTERS: { id: TaskCategory | 'all'; label: string }[] = [
  { id: 'all', label: '全部格式' },
  { id: 'archive', label: '镜像压缩' },
  { id: 'software', label: '应用程序' },
  { id: 'video', label: '影视视频' },
  { id: 'torrent', label: 'BT磁力' },
  { id: 'document', label: '文档' },
];

function fileBadge(category: TaskCategory, filename: string) {
  const ext = filename.split('.').pop()?.toUpperCase() || 'FILE';
  switch (category) {
    case 'video':
      return { ext, color: 'from-purple-500/15 to-indigo-500/15 border-purple-500/30 text-purple-600 dark:text-purple-400', icon: Film };
    case 'archive':
      return { ext, color: 'from-amber-500/15 to-orange-500/15 border-amber-500/30 text-amber-600 dark:text-amber-400', icon: Archive };
    case 'software':
      return { ext, color: 'from-blue-500/15 to-sky-500/15 border-blue-500/30 text-blue-600 dark:text-blue-400', icon: Package };
    case 'torrent':
      return { ext, color: 'from-teal-500/15 to-emerald-500/15 border-teal-500/30 text-teal-600 dark:text-teal-400', icon: Magnet };
    default:
      return { ext, color: 'from-emerald-500/15 to-teal-500/15 border-emerald-500/30 text-emerald-600 dark:text-emerald-400', icon: FileText };
  }
}

function toggleExpand(id: string) {
  expandedTaskId.value = expandedTaskId.value === id ? null : id;
}

async function copyUrl(task: DownloadTask) {
  if (!task.url) {
    store.addToast('该任务没有可复制的链接（旧任务记录可能缺少 URL）', 'error');
    return;
  }
  try {
    await navigator.clipboard.writeText(task.url);
    store.addToast('下载链接已复制到剪贴板', 'info');
  } catch {
    store.addToast('复制失败：剪贴板不可用', 'error');
  }
}

async function copyHash(hash: string) {
  await navigator.clipboard.writeText(hash);
  copiedHash.value = hash;
  setTimeout(() => (copiedHash.value = null), 1500);
}
</script>

<template>
  <div class="flex flex-1 flex-col overflow-hidden px-5 pb-5 select-none">
    <!-- 筛选条：状态 × 格式 -->
    <div class="mb-4 flex flex-wrap items-center justify-between gap-3 border-b border-slate-200 pb-2 dark:border-slate-800/80">
      <div class="flex items-center gap-1 rounded-xl border border-slate-200 bg-slate-200/60 p-1 text-xs dark:border-slate-800 dark:bg-slate-900">
        <button
          v-for="item in VIEW_FILTERS"
          :key="item.id"
          class="cursor-pointer rounded-lg px-3 py-1.5 font-medium transition-all"
          :class="
            store.viewFilter === item.id
              ? 'bg-white font-semibold text-slate-900 shadow-xs dark:bg-slate-800 dark:text-slate-100'
              : 'text-slate-600 hover:text-slate-900 dark:text-slate-400 dark:hover:text-slate-200'
          "
          @click="store.viewFilter = item.id"
        >
          {{ item.label }}
        </button>
      </div>

      <div class="flex items-center gap-1.5 overflow-x-auto text-xs">
        <button
          v-for="cat in CATEGORY_FILTERS"
          :key="cat.id"
          class="cursor-pointer rounded-lg px-2.5 py-1 text-xs transition-colors"
          :class="
            store.categoryFilter === cat.id
              ? 'border border-blue-500/30 bg-blue-500/15 font-semibold text-blue-600 dark:text-blue-400'
              : 'text-slate-600 hover:bg-slate-200/60 hover:text-slate-900 dark:text-slate-400 dark:hover:bg-slate-800/60 dark:hover:text-slate-200'
          "
          @click="store.categoryFilter = cat.id"
        >
          {{ cat.label }}
        </button>
      </div>
    </div>

    <!-- 任务卡片流 -->
    <div class="flex-1 space-y-3 overflow-y-auto pr-1">
      <div
        v-if="store.filteredTasks.length === 0"
        class="flex h-64 flex-col items-center justify-center rounded-2xl border border-dashed border-slate-300 bg-white/50 p-8 text-center dark:border-slate-800 dark:bg-transparent"
      >
        <ArchiveIcon class="mb-2 h-10 w-10 text-slate-400 dark:text-slate-600" />
        <div class="text-xs font-semibold text-slate-700 dark:text-slate-300">没有匹配的下载任务</div>
        <p class="mt-1 max-w-xs text-[11px] text-slate-400">点击上方“新建任务”按钮，体验高并发多线程极速拉取。</p>
      </div>

      <div
        v-for="task in store.filteredTasks"
        :key="task.id"
        class="overflow-hidden rounded-2xl border transition-all"
        @contextmenu.prevent="openCtx($event, task)"
        :class="
          task.status === 'downloading'
            ? 'border-blue-400/80 bg-white shadow-md ring-1 ring-blue-500/20 dark:border-blue-500/30 dark:bg-slate-900/70 dark:ring-blue-500/10'
            : task.status === 'completed'
              ? 'border-slate-200/90 bg-white shadow-2xs dark:border-slate-800/80 dark:bg-slate-900/80'
              : 'border-slate-200/90 bg-white/90 shadow-2xs dark:border-slate-800/60 dark:bg-slate-900/50'
        "
      >
        <!-- 卡片主面 -->
        <div class="flex flex-col justify-between gap-4 p-4 lg:flex-row lg:items-center">
          <!-- 左：徽标与名称 -->
          <div class="flex min-w-0 flex-1 items-center gap-3.5">
            <div
              class="flex h-11 w-11 shrink-0 flex-col items-center justify-center rounded-xl border bg-gradient-to-br shadow-xs"
              :class="fileBadge(task.category, task.name).color"
            >
              <component :is="fileBadge(task.category, task.name).icon" class="h-3.5 w-3.5" />
              <span class="mt-0.5 text-[9px] font-extrabold uppercase tracking-tighter font-mono">
                {{ fileBadge(task.category, task.name).ext.slice(0, 4) }}
              </span>
            </div>

            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-2">
                <h3 class="truncate text-xs font-semibold text-slate-900 dark:text-slate-100" :title="task.name">
                  {{ task.name }}
                </h3>
                <span
                  v-if="task.isTorrent"
                  class="rounded-md border border-teal-500/30 bg-teal-500/15 px-1.5 py-0.2 font-mono text-[9px] text-teal-600 dark:text-teal-400"
                >
                  BT
                </span>
                <span
                  v-if="task.useClash"
                  class="flex items-center gap-0.5 rounded-md border border-emerald-500/30 bg-emerald-500/15 px-1.5 py-0.2 font-mono text-[9px] font-semibold text-emerald-600 dark:text-emerald-400"
                  :title="`经 Clash 本地代理 127.0.0.1:${store.settings.proxyPort} 下载`"
                >
                  <ShieldCheck class="h-2.5 w-2.5" /> Clash
                </span>
              </div>
              <div class="mt-1 flex items-center gap-2 truncate font-mono text-[11px] text-slate-500 dark:text-slate-400">
                <span class="max-w-xs truncate">{{ task.url || task.savePath }}</span>
                <span>·</span>
                <span>{{ formatBytes(task.totalSize) }}</span>
                <span>·</span>
                <span>{{ task.threads }} 线程</span>
              </div>
            </div>
          </div>

          <!-- 中：分段流管 -->
          <div class="w-full space-y-1.5 lg:w-72">
            <div class="flex items-center justify-between font-mono text-xs tabular-nums">
              <div class="flex items-center gap-1.5">
                <span class="font-bold text-slate-800 dark:text-slate-200">
                  {{ task.totalSize > 0 ? Math.min(100, Math.floor((task.downloadedSize / task.totalSize) * 100)) + '%' : formatBytes(task.downloadedSize) }}
                </span>
                <span v-if="task.status === 'downloading' && task.speed > 0" class="text-[11px] font-semibold text-blue-600 dark:text-blue-400">
                  {{ formatSpeed(task.speed) }}
                </span>
                <span
                  v-if="task.status === 'completed'"
                  class="flex items-center gap-1 text-[11px] font-semibold text-emerald-600 dark:text-emerald-400"
                >
                  <CheckCircle2 class="h-3 w-3" /> 已完成
                </span>
                <span v-if="task.status === 'error'" class="text-[11px] font-semibold text-rose-500">
                  {{ task.errorMessage ? '失败' : '错误' }}
                </span>
              </div>
              <span class="text-[10px] text-slate-500 dark:text-slate-400">
                {{
                  task.status === 'downloading'
                    ? task.totalSize > 0
                      ? `剩 ${formatEta(task.totalSize - task.downloadedSize, task.speed)}`
                      : formatBytes(task.downloadedSize)
                    : formatBytes(task.downloadedSize)
                }}
              </span>
            </div>

            <!-- 总大小未知：不定态进度 -->
            <div
              v-if="task.totalSize <= 0 || task.segments.length === 0"
              class="relative h-3 w-full overflow-hidden rounded-lg border border-slate-200 bg-slate-100 dark:border-slate-700/40 dark:bg-slate-800"
              title="总大小未知（服务器未报告），显示已下载字节"
            >
              <div
                class="h-full rounded-xs bg-sky-500/70 transition-all duration-300"
                :class="task.status === 'downloading' ? 'animate-pulse' : ''"
                :style="{ width: task.status === 'downloading' ? '40%' : '100%' }"
              />
            </div>
            <!-- 已知大小：分段流管 -->
            <div
              v-else
              class="flex h-3 w-full gap-[2px] overflow-hidden rounded-lg border border-slate-200 bg-slate-100 p-[2px] dark:border-slate-700/40 dark:bg-slate-800"
            >
              <div
                v-for="seg in task.segments"
                :key="seg.id"
                class="relative h-full flex-1 overflow-hidden rounded-xs bg-slate-200/70 dark:bg-slate-700/40"
                :title="`线程 #${seg.id}: ${Math.min(100, Math.floor((seg.downloadedBytes / ((seg.endByte - seg.startByte) || 1)) * 100))}% (${formatBytes(seg.downloadedBytes)})`"
              >
                <div
                  class="h-full transition-all duration-300"
                  :class="
                    seg.downloadedBytes >= seg.endByte - seg.startByte
                      ? 'bg-blue-600 dark:bg-blue-500'
                      : seg.status === 'active'
                        ? 'animate-pulse bg-sky-500 dark:bg-sky-400'
                        : 'bg-transparent'
                  "
                  :style="{
                    width: `${Math.min(1, seg.downloadedBytes / ((seg.endByte - seg.startByte) || 1)) * 100}%`,
                  }"
                />
              </div>
            </div>
          </div>

          <!-- 右：行内操作 -->
          <div class="flex shrink-0 items-center gap-1 self-end lg:self-center">
            <button
              v-if="task.status === 'completed'"
              class="flex cursor-pointer items-center gap-1 rounded-lg border border-emerald-500/20 bg-emerald-500/10 px-2.5 py-1.5 text-xs font-semibold text-emerald-600 transition-colors hover:bg-emerald-500/20 dark:text-emerald-400"
              title="打开文件"
              @click="store.openFile(task.id)"
            >
              <FolderOpen class="h-3.5 w-3.5" /><span>打开</span>
            </button>
            <button
              v-else-if="task.status === 'error'"
              class="flex cursor-pointer items-center gap-1 rounded-lg border border-amber-500/20 bg-amber-500/10 px-2.5 py-1.5 text-xs font-semibold text-amber-600 transition-colors hover:bg-amber-500/20 dark:text-amber-400"
              @click="store.restartTask(task.id)"
            >
              <RotateCw class="h-3.5 w-3.5" /><span>重试</span>
            </button>
            <button
              v-else
              class="flex cursor-pointer items-center gap-1 rounded-lg px-3 py-1.5 text-xs font-semibold transition-all"
              :class="
                task.status === 'downloading'
                  ? 'border border-slate-200 bg-slate-100 text-slate-700 hover:bg-slate-200 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300 dark:hover:bg-slate-700'
                  : 'bg-blue-600 text-white shadow-xs hover:bg-blue-500'
              "
              :title="task.status === 'downloading' ? '暂停' : '继续'"
              @click="store.toggleTaskStatus(task.id)"
            >
              <Pause v-if="task.status === 'downloading'" class="h-3 w-3 fill-current" />
              <Play v-else class="h-3 w-3 fill-current" />
              <span>{{ task.status === 'downloading' ? '暂停' : '继续' }}</span>
            </button>

            <button
              title="打开文件夹"
              class="cursor-pointer rounded-lg p-1.5 text-slate-400 transition-colors hover:bg-slate-100 hover:text-slate-700 dark:hover:bg-slate-800 dark:hover:text-slate-200"
              @click.stop="store.openFolder(task.id)"
            >
              <FolderOpen class="h-3.5 w-3.5" />
            </button>

            <button
              title="复制原始链接"
              class="cursor-pointer rounded-lg p-1.5 text-slate-400 transition-colors hover:bg-slate-100 hover:text-slate-700 dark:hover:bg-slate-800 dark:hover:text-slate-200"
              @click="copyUrl(task)"
            >
              <Copy class="h-3.5 w-3.5" />
            </button>

            <button
              title="删除任务"
              class="cursor-pointer rounded-lg p-1.5 text-slate-400 transition-colors hover:bg-rose-50 hover:text-rose-600 dark:hover:bg-rose-950/30 dark:hover:text-rose-400"
              @click.stop="askDelete(task)"
            >
              <Trash2 class="h-3.5 w-3.5" />
            </button>

            <button
              :title="expandedTaskId === task.id ? '收起详情' : '展开多线程切片与文件元数据'"
              class="cursor-pointer rounded-lg p-1.5 text-slate-400 transition-colors hover:bg-slate-100 hover:text-slate-700 dark:hover:bg-slate-800 dark:hover:text-slate-200"
              @click="toggleExpand(task.id)"
            >
              <ChevronUp v-if="expandedTaskId === task.id" class="h-3.5 w-3.5" />
              <ChevronDown v-else class="h-3.5 w-3.5" />
            </button>
          </div>
        </div>

        <!-- 展开遥测架 -->
        <div
          v-if="expandedTaskId === task.id"
          class="animate-in space-y-3.5 border-t border-slate-200 bg-slate-50/80 px-5 py-4 text-xs duration-150 dark:border-slate-800/80 dark:bg-slate-950/40"
        >
          <div class="grid grid-cols-1 gap-3 md:grid-cols-3">
            <div class="rounded-xl border border-slate-200 bg-white p-2.5 shadow-2xs dark:border-slate-800 dark:bg-slate-900 dark:shadow-none">
              <span class="text-[10px] text-slate-400">Windows 本地保存目录</span>
              <div class="mt-0.5 truncate font-mono text-[11px] text-slate-800 dark:text-slate-200">
                {{ task.savePath }}\{{ task.name }}
              </div>
            </div>
            <div class="rounded-xl border border-slate-200 bg-white p-2.5 shadow-2xs dark:border-slate-800 dark:bg-slate-900 dark:shadow-none">
              <span class="text-[10px] text-slate-400">创建时间与连接池</span>
              <div class="mt-0.5 font-mono text-[11px] text-slate-800 dark:text-slate-200">
                {{ formatDate(task.createdAt) }} · {{ task.threads }} 个分块 Worker
              </div>
            </div>
            <div class="rounded-xl border border-slate-200 bg-white p-2.5 shadow-2xs dark:border-slate-800 dark:bg-slate-900 dark:shadow-none">
              <div class="flex items-center justify-between">
                <span class="text-[10px] text-slate-400">{{ task.errorMessage ? '错误信息' : '任务状态' }}</span>
                <button
                  v-if="task.url"
                  class="flex cursor-pointer items-center gap-1 text-[10px] text-blue-600 hover:underline dark:text-blue-400"
                  @click="copyUrl(task)"
                >
                  <Check v-if="copiedHash === task.url" class="h-3 w-3 text-emerald-500" />
                  <Copy v-else class="h-3 w-3" />
                  <span>{{ copiedHash === task.url ? '已复制' : '复制链接' }}</span>
                </button>
              </div>
              <div class="mt-0.5 truncate font-mono text-[10px]" :class="task.errorMessage ? 'text-rose-500' : 'text-slate-500'">
                {{ task.errorMessage || '下载通道正常' }}
              </div>
            </div>
          </div>

          <!-- 分段矩阵 -->
          <div v-if="task.segments.length" class="space-y-1.5">
            <div class="flex items-center justify-between text-[11px] text-slate-500 dark:text-slate-400">
              <span>并行分块 Worker 状态分布 ({{ task.segments.length }} 线程)</span>
              <span class="font-mono tabular-nums text-slate-500">
                {{ task.segments.filter((s) => s.status === 'completed').length }}/{{ task.segments.length }} 已交付
              </span>
            </div>
            <div class="grid grid-cols-4 gap-1 sm:grid-cols-8 lg:grid-cols-16">
              <div
                v-for="seg in task.segments"
                :key="seg.id"
                class="rounded border p-1 text-center transition-all"
                :class="
                  seg.status === 'completed'
                    ? 'border-blue-500/30 bg-blue-500/10 text-blue-600 dark:text-blue-400'
                    : seg.status === 'active'
                      ? 'border-sky-400 bg-sky-500/15 font-bold text-sky-600 dark:text-sky-300'
                      : 'border-slate-200 bg-slate-100 text-slate-500 dark:border-transparent dark:bg-slate-800 dark:text-slate-500'
                "
                :title="`分块 #${seg.id}: ${seg.status === 'completed' ? '已完成' : seg.status === 'active' ? '高速传输' : '就绪'}`"
              >
                <div class="text-[9px] font-mono leading-none">#{{ seg.id }}</div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 右键菜单：仅打开文件 / 打开文件夹 -->
    <div
      v-if="ctxMenu"
      class="fixed z-50 w-44 animate-in rounded-lg border border-slate-200 bg-white/95 py-1 text-xs shadow-xl backdrop-blur-xl duration-100 dark:border-slate-700/80 dark:bg-slate-900/95"
      :style="{ left: `${ctxMenu.x}px`, top: `${ctxMenu.y}px` }"
      @click.stop
    >
      <button
        class="flex w-full cursor-pointer items-center gap-2 px-3 py-1.5 text-left text-slate-700 transition-colors hover:bg-blue-600 hover:text-white dark:text-slate-300"
        @click.stop="ctxOpenFile(ctxMenu.task)"
      >
        <FileText class="h-3 w-3" /><span>打开文件</span>
      </button>
      <button
        class="flex w-full cursor-pointer items-center gap-2 px-3 py-1.5 text-left text-slate-700 transition-colors hover:bg-blue-600 hover:text-white dark:text-slate-300"
        @click.stop="ctxOpenFolder(ctxMenu.task)"
      >
        <FolderOpen class="h-3 w-3" /><span>打开文件夹</span>
      </button>
      <button
        v-if="!ctxMenu.task.isTorrent"
        class="flex w-full cursor-pointer items-center gap-2 px-3 py-1.5 text-left text-slate-700 transition-colors hover:bg-blue-600 hover:text-white dark:text-slate-300"
        @click.stop="ctxEdit(ctxMenu.task)"
      >
        <Pencil class="h-3 w-3" /><span>编辑任务</span>
      </button>
      <button
        class="flex w-full cursor-pointer items-center gap-2 px-3 py-1.5 text-left text-slate-700 transition-colors hover:bg-blue-600 hover:text-white dark:text-slate-300"
        @click.stop="store.restartTask(ctxMenu.task.id); closeCtx()"
      >
        <RotateCw class="h-3 w-3" /><span>重新下载</span>
      </button>
      <button
        class="flex w-full cursor-pointer items-center gap-2 px-3 py-1.5 text-left text-slate-700 transition-colors hover:bg-blue-600 hover:text-white dark:text-slate-300"
        @click.stop="copyUrl(ctxMenu.task); closeCtx()"
      >
        <Copy class="h-3 w-3" /><span>复制下载链接</span>
      </button>
      <button
        class="flex w-full cursor-pointer items-center gap-2 px-3 py-1.5 text-left text-slate-700 transition-colors hover:bg-blue-600 hover:text-white dark:text-slate-300"
        @click.stop="store.toggleTaskClash(ctxMenu.task); closeCtx()"
      >
        <ShieldCheck class="h-3 w-3" /><span>{{ ctxMenu.task.useClash ? '切换为直连' : '切换为 Clash 代理' }}</span>
      </button>
    </div>

    <!-- 删除确认窗 -->
    <div
      v-if="deleteConfirm"
      class="fixed inset-0 z-50 flex select-none items-center justify-center bg-slate-900/50 p-4 backdrop-blur-xs"
      @mousedown.self="deleteConfirm = null"
    >
      <div
        class="w-full max-w-sm rounded-xl border border-slate-200 bg-white p-5 text-sm shadow-2xl transition-all dark:border-slate-800 dark:bg-slate-900"
      >
        <h3 class="mb-2 font-bold text-slate-900 dark:text-slate-100">删除下载任务</h3>
        <p class="mb-3 text-xs leading-relaxed text-slate-500 dark:text-slate-400">
          确定删除「{{ deleteConfirm.task.name }}」？任务的进度记录将被移除。
        </p>
        <label
          class="mb-4 flex cursor-pointer items-center gap-2 rounded-md border border-slate-200 bg-slate-50 px-3 py-2 text-xs text-slate-700 dark:border-slate-700 dark:bg-slate-800/60 dark:text-slate-300"
          :class="deleteConfirm.task.isTorrent ? 'pointer-events-none opacity-50' : ''"
          :title="deleteConfirm.task.isTorrent ? 'BT 任务文件结构不同，暂不支持源文件删除' : ''"
        >
          <input v-model="deleteConfirm.deleteFiles" type="checkbox" class="h-4 w-4 rounded text-rose-600 focus:ring-0" />
          <span>同时删除已下载的源文件</span>
        </label>
        <div class="flex justify-end gap-2.5">
          <button
            class="cursor-pointer rounded-md px-4 py-2 text-xs font-medium text-slate-600 transition-colors hover:bg-slate-100 dark:text-slate-400 dark:hover:bg-slate-800"
            @click="deleteConfirm = null"
          >
            取消
          </button>
          <button
            class="cursor-pointer rounded-md bg-rose-600 px-4 py-2 text-xs font-semibold text-white transition-colors hover:bg-rose-700"
            @click="confirmDelete"
          >
            删除
          </button>
        </div>
      </div>
    </div>

    <EditTaskModal :task="editingTask" @close="editingTask = null" />
  </div>
</template>
