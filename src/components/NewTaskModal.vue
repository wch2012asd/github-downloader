<script setup lang="ts">
// 新建下载任务：常规链接 / 磁力 BT / 批量导入 三标签
import { ref, watch } from 'vue';
import { Clipboard, Download, Folder, Link2, ListPlus, Magnet, X } from 'lucide-vue-next';
import { useAppStore } from '../stores/app';
import { api, isTauri } from '../lib/bridge';

const props = defineProps<{ isOpen: boolean }>();
const emit = defineEmits<{ (e: 'close'): void }>();

const store = useAppStore();
const tab = ref<'url' | 'magnet' | 'batch'>('url');
const url = ref('');
const fileName = ref('');
const savePath = ref(store.settings.defaultSavePath);
const threads = ref(store.settings.defaultThreads);
const autoStart = ref(true);
const useClash = ref(false);
const batchUrls = ref('');
const errorMsg = ref('');
// fileName 当前值是否属于「自动推导」：自动值可被新链接覆盖，用户手动编辑过则保护
const fileNameAuto = ref(true);

watch(
  () => props.isOpen,
  (open) => {
    if (open) {
      // 每次打开重置表单（上次的链接/文件名不能残留，否则自动取名逻辑被旧值挡住）
      tab.value = 'url';
      url.value = '';
      fileName.value = '';
      fileNameAuto.value = true;
      batchUrls.value = '';
      savePath.value = store.settings.defaultSavePath;
      threads.value = store.settings.defaultThreads;
      errorMsg.value = '';
    }
  },
);

function handleUrlChange(val: string) {
  url.value = val;
  errorMsg.value = '';
  if (fileNameAuto.value) {
    const cleanUrl = val.split('?')[0].split('#')[0];
    const last = cleanUrl.split('/').pop() ?? '';
    if (last && last.includes('.')) {
      fileName.value = decodeURIComponent(last);
      fileNameAuto.value = true;
    }
  }
}

async function pasteClipboard() {
  try {
    const text = await navigator.clipboard.readText();
    if (text) handleUrlChange(text.trim());
  } catch {
    store.addToast('剪贴板读取失败，请手动粘贴', 'error');
  }
}

async function browseFolder() {
  if (!isTauri) {
    savePath.value = 'C:\\Users\\Administrator\\Downloads';
    return;
  }
  try {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const picked = await open({ directory: true, multiple: false, title: '选择下载目录' });
    if (typeof picked === 'string' && picked) savePath.value = picked;
  } catch (e) {
    store.addToast(`目录选择失败: ${String(e)}`, 'error');
  }
}

async function submit() {
  try {
    if (tab.value === 'batch') {
      const lines = batchUrls.value.split('\n').map((l) => l.trim()).filter((l) => l.length > 0);
      if (lines.length === 0) {
        errorMsg.value = '请至少输入一条有效的下载链接';
        return;
      }
      if (!savePath.value.trim()) {
        errorMsg.value = '请填写存储目录';
        return;
      }
      for (const line of lines) {
        const name = line.split('?')[0].split('/').pop() || 'downloaded_file';
        await store.addTask({
          name,
          url: line,
          threads: threads.value,
          savePath: savePath.value,
          autoStart: autoStart.value,
          useClash: useClash.value,
        });
      }
      store.addToast(`已批量创建 ${lines.length} 个任务`, 'success');
      emit('close');
      return;
    }

    if (!url.value.trim()) {
      errorMsg.value = '请输入有效的下载地址或磁力链接';
      return;
    }
    if (!savePath.value.trim()) {
      errorMsg.value = '请填写存储目录';
      return;
    }

    const name =
      fileName.value.trim() ||
      (tab.value === 'magnet' ? 'BT_Magnet_Task' : url.value.split('?')[0].split('/').pop() || 'downloaded_package');
    const isTorrent = tab.value === 'magnet' || url.value.startsWith('magnet:') || name.endsWith('.torrent');

    await store.addTask({
      name,
      url: url.value.trim(),
      threads: threads.value,
      savePath: savePath.value,
      isTorrent,
      autoStart: autoStart.value,
      useClash: useClash.value,
    });
    store.addToast(
      isTorrent ? `已成功创建 BT 任务: ${name}` : `已成功创建任务: ${name}${useClash.value ? '（经 Clash 代理）' : ''}`,
      'success',
    );
    emit('close');
  } catch (e) {
    errorMsg.value = `创建失败: ${String(e)}`;
    store.addToast(`创建任务失败: ${String(e)}`, 'error');
  }
}
</script>

<template>
  <div
    v-if="isOpen"
    class="fixed inset-0 z-50 flex select-none items-center justify-center bg-slate-900/50 p-4 backdrop-blur-xs"
    @mousedown.self="emit('close')"
  >
    <div
      class="flex max-h-[90vh] w-full max-w-lg flex-col overflow-hidden rounded-xl border border-slate-200 bg-white shadow-2xl transition-all dark:border-slate-800 dark:bg-slate-900"
    >
      <div class="flex items-center justify-between border-b border-slate-200 px-5 py-4 dark:border-slate-800">
        <h2 class="flex items-center gap-2 text-sm font-bold text-slate-900 dark:text-slate-100">
          <Download class="h-4 w-4 text-blue-600" />
          <span>新建下载任务</span>
        </h2>
        <button
          class="cursor-pointer rounded-md p-1 text-slate-400 transition-colors hover:text-slate-700 dark:hover:text-slate-200"
          @click="emit('close')"
        >
          <X class="h-4 w-4" />
        </button>
      </div>

      <div class="flex gap-2 border-b border-slate-200 px-5 pt-2 text-xs dark:border-slate-800">
        <button
          v-for="t in [
            { id: 'url', label: '常规链接', icon: Link2 },
            { id: 'magnet', label: '磁力链 / BT', icon: Magnet },
            { id: 'batch', label: '批量导入', icon: ListPlus },
          ]"
          :key="t.id"
          type="button"
          class="flex items-center gap-1.5 border-b-2 px-2 pb-2.5 font-medium transition-colors"
          :class="
            tab === t.id
              ? 'border-blue-600 font-semibold text-blue-600 dark:border-blue-600 dark:text-blue-400'
              : 'border-transparent text-slate-500 hover:text-slate-900 dark:hover:text-slate-300'
          "
          @click="tab = t.id as 'url' | 'magnet' | 'batch'"
        >
          <component :is="t.icon" class="h-3.5 w-3.5" />
          <span>{{ t.label }}</span>
        </button>
      </div>

      <form class="space-y-4 overflow-y-auto p-5 text-xs" @submit.prevent="submit">
        <div v-if="tab === 'batch'" class="space-y-1.5">
          <label class="text-[11px] font-semibold text-slate-600 dark:text-slate-400">批量 URL 链接列表 (每行一条)</label>
          <textarea
            v-model="batchUrls"
            rows="4"
            placeholder="https://example.com/file1.zip&#10;https://example.com/file2.zip"
            class="w-full rounded-md border border-slate-200 bg-slate-50 p-2.5 font-mono text-xs text-slate-900 outline-none placeholder:text-slate-400 focus:border-blue-500 dark:border-slate-700 dark:bg-slate-800/60 dark:text-slate-100"
          />
        </div>
        <template v-else>
          <div class="space-y-1.5">
            <div class="flex items-center justify-between">
              <label class="text-[11px] font-semibold text-slate-600 dark:text-slate-400">
                {{ tab === 'magnet' ? '磁力链接 (magnet:?xt=...) 或 Torrent' : '下载链接 (HTTP / HTTPS)' }}
              </label>
              <button
                type="button"
                class="flex cursor-pointer items-center gap-1 text-[11px] text-blue-600 hover:underline dark:text-blue-400"
                @click="pasteClipboard"
              >
                <Clipboard class="h-3 w-3" /><span>粘贴剪贴板</span>
              </button>
            </div>
            <input
              type="text"
              :value="url"
              :placeholder="tab === 'magnet' ? 'magnet:?xt=urn:btih:...' : 'https://example.com/release/app-installer-x64.exe'"
              class="h-9 w-full rounded-md border border-slate-200 bg-slate-50 px-3 font-mono text-xs text-slate-900 outline-none placeholder:text-slate-400 focus:border-blue-500 dark:border-slate-700 dark:bg-slate-800/60 dark:text-slate-100"
              @input="handleUrlChange(($event.target as HTMLInputElement).value)"
            />
          </div>

          <div class="space-y-1.5">
            <label class="text-[11px] font-semibold text-slate-600 dark:text-slate-400">文件重命名 (可选)</label>
            <input
              v-model="fileName"
              type="text"
              @input="fileNameAuto = false"
              placeholder="留空自动使用服务器远程文件名"
              class="h-9 w-full rounded-md border border-slate-200 bg-slate-50 px-3 text-xs text-slate-900 outline-none placeholder:text-slate-400 focus:border-blue-500 dark:border-slate-700 dark:bg-slate-800/60 dark:text-slate-100"
            />
          </div>
        </template>

        <div class="space-y-1.5">
          <label class="text-[11px] font-semibold text-slate-600 dark:text-slate-400">存储目标目录</label>
          <div class="flex items-center gap-2">
            <input
              v-model="savePath"
              type="text"
              class="h-9 flex-1 rounded-md border border-slate-200 bg-slate-50 px-3 font-mono text-xs text-slate-900 outline-none focus:border-blue-500 dark:border-slate-700 dark:bg-slate-800/60 dark:text-slate-100"
            />
            <button
              type="button"
              title="选择下载文件夹"
              class="flex h-9 items-center gap-1.5 rounded-md border border-slate-200 bg-slate-100 px-3 text-slate-700 transition-colors hover:bg-slate-200 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300 dark:hover:bg-slate-700"
              @click="browseFolder"
            >
              <Folder class="h-3.5 w-3.5" /><span>浏览</span>
            </button>
          </div>
        </div>

        <div class="grid grid-cols-2 gap-4 pt-1">
          <div class="space-y-1.5">
            <div class="flex items-center justify-between">
              <label class="text-[11px] font-semibold text-slate-600 dark:text-slate-400">并发分块连接数</label>
              <span class="font-mono font-semibold text-blue-600 dark:text-blue-400">{{ threads }} 线程</span>
            </div>
            <input v-model.number="threads" type="range" min="1" max="32" step="1" class="w-full cursor-pointer accent-blue-600" />
          </div>
          <div class="flex items-center gap-2 pt-4">
            <input
              id="autoStartCheck"
              v-model="autoStart"
              type="checkbox"
              class="h-4 w-4 cursor-pointer rounded border-slate-300 text-blue-600 focus:ring-0 dark:border-slate-700"
            />
            <label for="autoStartCheck" class="cursor-pointer text-xs text-slate-700 dark:text-slate-300">创建后立即开始下载</label>
          </div>
        </div>

        <!-- Clash 通道（v2 能力回归：仅 HTTP 任务；BT 走 DHT/Peer 无代理语义） -->
        <label
          v-if="tab !== 'magnet'"
          class="flex cursor-pointer items-center gap-2 rounded-md border border-slate-200 bg-slate-50 px-3 py-2 dark:border-slate-700 dark:bg-slate-800/60"
        >
          <input
            v-model="useClash"
            type="checkbox"
            class="h-4 w-4 cursor-pointer rounded border-slate-300 text-blue-600 focus:ring-0 dark:border-slate-700"
          />
          <span class="text-xs text-slate-700 dark:text-slate-300">
            通过 Clash 本地代理下载
            <span class="text-slate-400">(127.0.0.1:{{ store.settings.proxyPort }})</span>
          </span>
        </label>

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
            type="submit"
            class="cursor-pointer rounded-md bg-blue-600 px-5 py-2 text-xs font-semibold text-white shadow-xs transition-colors hover:bg-blue-700"
          >
            立即下载
          </button>
        </div>
      </form>
    </div>
  </div>
</template>
