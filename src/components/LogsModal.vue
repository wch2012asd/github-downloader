<script setup lang="ts">
// 内核运行日志：级别筛选 / 复制 / 清空
import { computed, ref } from 'vue';
import { Check, Copy, Terminal, Trash2, X } from 'lucide-vue-next';
import { useAppStore } from '../stores/app';

defineProps<{ isOpen: boolean }>();
const emit = defineEmits<{ (e: 'close'): void; (e: 'clearLogs'): void }>();

const store = useAppStore();
const filterLevel = ref<'all' | 'info' | 'warn' | 'error'>('all');
const copied = ref(false);

const filteredLogs = computed(() =>
  store.logs.filter((l) => filterLevel.value === 'all' || l.level === filterLevel.value),
);

async function copyLogs() {
  const text = filteredLogs.value.map((l) => `[${l.timestamp}] [${l.level.toUpperCase()}] ${l.message}`).join('\n');
  await navigator.clipboard.writeText(text);
  copied.value = true;
  setTimeout(() => (copied.value = false), 1500);
}
</script>

<template>
  <div
    v-if="isOpen"
    class="fixed inset-0 z-50 flex select-none items-center justify-center bg-slate-900/50 p-4 backdrop-blur-xs"
    @mousedown.self="emit('close')"
  >
    <div
      class="flex h-[520px] w-full max-w-2xl flex-col overflow-hidden rounded-xl border border-slate-200 bg-white shadow-2xl transition-all dark:border-slate-800 dark:bg-slate-900"
    >
      <div class="flex items-center justify-between border-b border-slate-200 px-5 py-3.5 dark:border-slate-800">
        <div class="flex items-center gap-2">
          <Terminal class="h-4 w-4 text-blue-600" />
          <h2 class="text-sm font-bold text-slate-900 dark:text-slate-100">下载引擎内核运行日志与网络诊断</h2>
        </div>
        <button
          class="cursor-pointer rounded-md p-1 text-slate-400 transition-colors hover:text-slate-700 dark:hover:text-slate-200"
          @click="emit('close')"
        >
          <X class="h-4 w-4" />
        </button>
      </div>

      <div
        class="flex items-center justify-between border-b border-slate-200 bg-slate-50 px-5 py-2.5 text-xs dark:border-slate-800 dark:bg-slate-900/60"
      >
        <div class="flex items-center gap-1">
          <span class="mr-1 text-[11px] text-slate-400">日志级别:</span>
          <button
            v-for="lvl in ['all', 'info', 'warn', 'error']"
            :key="lvl"
            class="rounded px-2 py-0.5 text-[11px] font-medium transition-colors"
            :class="
              filterLevel === lvl
                ? 'bg-slate-900 text-white dark:bg-slate-700'
                : 'text-slate-600 hover:bg-slate-200 dark:text-slate-400 dark:hover:bg-slate-800'
            "
            @click="filterLevel = lvl as 'all' | 'info' | 'warn' | 'error'"
          >
            {{ lvl === 'all' ? '全部' : lvl.toUpperCase() }}
          </button>
        </div>
        <div class="flex items-center gap-2">
          <button
            class="flex cursor-pointer items-center gap-1 text-[11px] text-slate-600 transition-colors hover:text-slate-900 dark:text-slate-400 dark:hover:text-slate-100"
            @click="copyLogs"
          >
            <Check v-if="copied" class="h-3.5 w-3.5 text-emerald-500" />
            <Copy v-else class="h-3.5 w-3.5" />
            <span>{{ copied ? '已复制' : '复制日志' }}</span>
          </button>
          <button
            class="flex cursor-pointer items-center gap-1 text-[11px] text-slate-600 transition-colors hover:text-rose-600 dark:text-slate-400 dark:hover:text-rose-400"
            @click="emit('clearLogs')"
          >
            <Trash2 class="h-3.5 w-3.5" /><span>清空</span>
          </button>
        </div>
      </div>

      <div class="flex-1 space-y-1 overflow-y-auto select-text bg-slate-950 p-4 font-mono text-xs">
        <div v-if="filteredLogs.length === 0" class="py-8 text-center text-slate-600">暂无符合条件的运行日志</div>
        <div v-for="log in filteredLogs" :key="log.id" class="flex items-start gap-2 leading-relaxed">
          <span class="shrink-0 tabular-nums text-slate-500">[{{ log.timestamp }}]</span>
          <span
            class="shrink-0 font-semibold"
            :class="log.level === 'error' ? 'text-rose-400' : log.level === 'warn' ? 'text-amber-400' : 'text-sky-400'"
          >
            [{{ log.level.toUpperCase() }}]
          </span>
          <span :class="log.level === 'error' ? 'text-rose-200' : log.level === 'warn' ? 'text-amber-100' : 'text-slate-300'">
            {{ log.message }}
          </span>
        </div>
      </div>

      <div
        class="flex items-center justify-between border-t border-slate-200 bg-slate-50 px-5 py-2.5 text-xs text-slate-400 dark:border-slate-800 dark:bg-slate-900/80"
      >
        <span>VortexCore Engine · Rust + tokio 多线程内核</span>
        <button
          class="cursor-pointer rounded px-4 py-1 text-xs text-slate-600 transition-colors hover:bg-slate-200 dark:text-slate-300 dark:hover:bg-slate-800"
          @click="emit('close')"
        >
          关闭
        </button>
      </div>
    </div>
  </div>
</template>
