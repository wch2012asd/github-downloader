<script setup lang="ts">
// 编辑任务（仅 HTTP）：文件名 / 保存目录 / 线程数 / Clash 通道
import { ref, watch } from 'vue';
import { Folder, Pencil, X } from 'lucide-vue-next';
import { useAppStore } from '../stores/app';
import { api, isTauri } from '../lib/bridge';
import type { DownloadTask } from '../types';

const props = defineProps<{ task: DownloadTask | null }>();
const emit = defineEmits<{ (e: 'close'): void }>();

const store = useAppStore();
const name = ref('');
const savePath = ref('');
const threads = ref(8);
const useClash = ref(false);
const saving = ref(false);
const errorMsg = ref('');

watch(
  () => props.task,
  (t) => {
    if (t) {
      name.value = t.name;
      savePath.value = t.savePath;
      threads.value = t.threads;
      useClash.value = !!t.useClash;
      errorMsg.value = '';
      saving.value = false;
    }
  },
  { immediate: true },
);

async function browseFolder() {
  if (!isTauri) return;
  try {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const picked = await open({ directory: true, multiple: false, title: '选择保存目录' });
    if (typeof picked === 'string' && picked) savePath.value = picked;
  } catch (e) {
    store.addToast(`目录选择失败: ${String(e)}`, 'error');
  }
}

async function save() {
  if (!props.task) return;
  if (!name.value.trim()) {
    errorMsg.value = '文件名不能为空';
    return;
  }
  if (!savePath.value.trim()) {
    errorMsg.value = '保存目录不能为空';
    return;
  }
  saving.value = true;
  errorMsg.value = '';
  try {
    await api.updateTask({
      id: props.task.id,
      name: name.value.trim(),
      saveDir: savePath.value,
      segments: threads.value,
      useClash: useClash.value,
    });
    await store.loadFromBackend();
    store.addLog(`任务已更新: ${name.value.trim()}`, 'info', props.task.id);
    store.addToast('任务已更新', 'success');
    emit('close');
  } catch (e) {
    errorMsg.value = `保存失败: ${String(e)}`;
    store.addToast(`保存失败: ${String(e)}`, 'error');
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <div
    v-if="task"
    class="fixed inset-0 z-50 flex select-none items-center justify-center bg-slate-900/50 p-4 backdrop-blur-xs"
    @mousedown.self="emit('close')"
  >
    <div
      class="flex w-full max-w-md flex-col overflow-hidden rounded-xl border border-slate-200 bg-white shadow-2xl transition-all dark:border-slate-800 dark:bg-slate-900"
    >
      <div class="flex items-center justify-between border-b border-slate-200 px-5 py-4 dark:border-slate-800">
        <h2 class="flex items-center gap-2 text-sm font-bold text-slate-900 dark:text-slate-100">
          <Pencil class="h-4 w-4 text-blue-600" />
          <span>编辑任务</span>
        </h2>
        <button
          class="cursor-pointer rounded-md p-1 text-slate-400 transition-colors hover:text-slate-700 dark:hover:text-slate-200"
          @click="emit('close')"
        >
          <X class="h-4 w-4" />
        </button>
      </div>

      <div class="space-y-4 p-5 text-xs">
        <div class="space-y-1.5">
          <label class="text-[11px] font-semibold text-slate-600 dark:text-slate-400">文件名</label>
          <input
            v-model="name"
            type="text"
            class="h-9 w-full rounded-md border border-slate-200 bg-slate-50 px-3 text-xs text-slate-900 outline-none focus:border-blue-500 dark:border-slate-700 dark:bg-slate-800/60 dark:text-slate-100"
          />
        </div>

        <div class="space-y-1.5">
          <label class="text-[11px] font-semibold text-slate-600 dark:text-slate-400">保存目录</label>
          <div class="flex items-center gap-2">
            <input
              v-model="savePath"
              type="text"
              class="h-9 flex-1 rounded-md border border-slate-200 bg-slate-50 px-3 font-mono text-xs text-slate-900 outline-none focus:border-blue-500 dark:border-slate-700 dark:bg-slate-800/60 dark:text-slate-100"
            />
            <button
              type="button"
              class="flex h-9 items-center gap-1.5 rounded-md border border-slate-200 bg-slate-100 px-3 text-slate-700 transition-colors hover:bg-slate-200 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300 dark:hover:bg-slate-700"
              @click="browseFolder"
            >
              <Folder class="h-3.5 w-3.5" /><span>浏览</span>
            </button>
          </div>
        </div>

        <div class="grid grid-cols-2 gap-4">
          <div class="space-y-1.5">
            <div class="flex items-center justify-between">
              <label class="text-[11px] font-semibold text-slate-600 dark:text-slate-400">并发分块连接数</label>
              <span class="font-mono font-semibold text-blue-600 dark:text-blue-400">{{ threads }} 线程</span>
            </div>
            <input v-model.number="threads" type="range" min="1" max="32" step="1" class="w-full cursor-pointer accent-blue-600" />
          </div>
          <div class="flex items-center gap-2 pt-4">
            <input
              id="editUseClash"
              v-model="useClash"
              type="checkbox"
              class="h-4 w-4 cursor-pointer rounded border-slate-300 text-blue-600 focus:ring-0 dark:border-slate-700"
            />
            <label for="editUseClash" class="cursor-pointer text-xs text-slate-700 dark:text-slate-300">通过 Clash 代理下载</label>
          </div>
        </div>

        <p class="rounded-md border border-amber-200 bg-amber-50 p-2.5 text-[11px] leading-relaxed text-amber-700 dark:border-amber-500/30 dark:bg-amber-500/10 dark:text-amber-300">
          修改文件名、保存目录或线程数会重新开始下载（分段文件与路径绑定）；仅切换 Clash 通道可保留断点续传。
        </p>

        <div
          v-if="errorMsg"
          class="rounded border border-rose-200 bg-rose-50 p-2 text-xs text-rose-600 dark:border-rose-900 dark:bg-rose-950/40 dark:text-rose-400"
        >
          {{ errorMsg }}
        </div>

        <div class="flex items-center justify-end gap-2.5 border-t border-slate-200 pt-3 dark:border-slate-800">
          <button
            type="button"
            class="cursor-pointer rounded-md px-4 py-2 text-xs font-medium text-slate-600 transition-colors hover:bg-slate-100 dark:text-slate-400 dark:hover:bg-slate-800"
            @click="emit('close')"
          >
            取消
          </button>
          <button
            type="button"
            :disabled="saving"
            class="cursor-pointer rounded-md bg-blue-600 px-5 py-2 text-xs font-semibold text-white shadow-xs transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-60"
            @click="save"
          >
            {{ saving ? '保存中…' : '保存' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>