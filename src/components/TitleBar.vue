<script setup lang="ts">
// 标题栏：品牌 + Fluent 菜单（文件/任务/工具/帮助）+ Spotlight 搜索 + 主题切换 + 窗口控制
// Tauri 模式下窗口按钮接 getCurrentWindow；浏览器 dev 模式按钮置灰无操作
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { Search, Moon, Sun, Minus, Square, X, Check } from 'lucide-vue-next';
import { useAppStore } from '../stores/app';
import { isTauri } from '../lib/bridge';

defineProps<{ matchCount?: number }>();

const searchQuery = defineModel<string>('searchQuery', { default: '' });

const emit = defineEmits<{
  (e: 'openSettings'): void;
  (e: 'openLogs'): void;
  (e: 'openAbout'): void;
  (e: 'newTask'): void;
  (e: 'resumeAll'): void;
  (e: 'pauseAll'): void;
  (e: 'clearCompleted'): void;
}>();

const store = useAppStore();
const activeMenu = ref<string | null>(null);
const searchInput = ref<HTMLInputElement | null>(null);
const navRef = ref<HTMLElement | null>(null);

let win: {
  minimize: () => void;
  toggleMaximize: () => void;
  close: () => void;
} | null = null;

onMounted(async () => {
  const onKey = (e: KeyboardEvent) => {
    // Ctrl+F 聚焦搜索（设计稿交互）
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
      e.preventDefault();
      searchInput.value?.focus();
    }
  };
  const onClick = (e: MouseEvent) => {
    if (navRef.value && !navRef.value.contains(e.target as Node)) activeMenu.value = null;
  };
  window.addEventListener('keydown', onKey);
  document.addEventListener('mousedown', onClick);
  onBeforeUnmount(() => {
    window.removeEventListener('keydown', onKey);
    document.removeEventListener('mousedown', onClick);
  });

  if (isTauri) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const w = getCurrentWindow();
    win = {
      minimize: () => void w.minimize(),
      toggleMaximize: () => void w.toggleMaximize(),
      close: () => void w.close(),
    };
  }
});

function closeMenu() {
  activeMenu.value = null;
}

function exec(fn: () => void) {
  fn();
  closeMenu();
}
</script>

<template>
  <header
    data-tauri-drag-region
    class="z-50 flex h-10 shrink-0 select-none items-center justify-between border-b border-slate-200 bg-white/95 px-3 text-slate-700 backdrop-blur-xl transition-colors dark:border-slate-800/80 dark:bg-slate-950/90 dark:text-slate-200"
  >
    <!-- 左：品牌 + 菜单 -->
    <div class="flex items-center gap-3">
      <button
        class="group flex cursor-pointer items-center gap-2 text-left"
        title="关于 极涌 VortexDL"
        @click="emit('openAbout')"
      >
        <img src="/vortex-icon.png" alt="VortexDL" class="h-4 w-4 rounded-xs object-cover shadow-xs transition-transform group-hover:scale-110" />
        <span
          class="text-xs font-bold tracking-tight text-slate-900 transition-colors group-hover:text-blue-600 dark:text-white dark:group-hover:text-sky-400"
        >
          极涌 VortexDL
        </span>
      </button>

      <div class="hidden h-3 w-px bg-slate-300 dark:bg-slate-800 sm:block" />

      <nav ref="navRef" class="flex items-center gap-0.5 text-xs text-slate-600 dark:text-slate-300">
        <!-- 文件 -->
        <div class="relative">
          <button
            class="cursor-pointer rounded-md px-2.5 py-1 text-[11px] transition-colors hover:bg-slate-100 hover:text-slate-900 dark:hover:bg-slate-800/80 dark:hover:text-white"
            :class="activeMenu === 'file' ? 'bg-slate-200 text-slate-900 dark:bg-slate-800 dark:text-white' : ''"
            @click="activeMenu = activeMenu === 'file' ? null : 'file'"
          >
            文件
          </button>
          <div
            v-if="activeMenu === 'file'"
            class="absolute left-0 top-8 z-50 w-44 animate-in rounded-lg border border-slate-200 bg-white/95 py-1 text-xs shadow-xl backdrop-blur-xl duration-100 dark:border-slate-700/80 dark:bg-slate-900/95"
          >
            <button class="menu-item" @click="exec(() => emit('newTask'))">
              <span>新建任务...</span><span class="shortcut">Ctrl+N</span>
            </button>
          </div>
        </div>

        <!-- 任务 -->
        <div class="relative">
          <button
            class="cursor-pointer rounded-md px-2.5 py-1 text-[11px] transition-colors hover:bg-slate-100 hover:text-slate-900 dark:hover:bg-slate-800/80 dark:hover:text-white"
            :class="activeMenu === 'task' ? 'bg-slate-200 text-slate-900 dark:bg-slate-800 dark:text-white' : ''"
            @click="activeMenu = activeMenu === 'task' ? null : 'task'"
          >
            任务
          </button>
          <div
            v-if="activeMenu === 'task'"
            class="absolute left-0 top-8 z-50 w-44 animate-in rounded-lg border border-slate-200 bg-white/95 py-1 text-xs shadow-xl backdrop-blur-xl duration-100 dark:border-slate-700/80 dark:bg-slate-900/95"
          >
            <button class="menu-item" @click="exec(() => emit('resumeAll'))">
              <span>全部开始</span><span class="shortcut">Ctrl+R</span>
            </button>
            <button class="menu-item" @click="exec(() => emit('pauseAll'))">
              <span>全部暂停</span><span class="shortcut">Ctrl+P</span>
            </button>
            <div class="my-1 border-t border-slate-100 dark:border-slate-800" />
            <button class="menu-item" @click="exec(() => emit('clearCompleted'))">
              <span>清除已完成记录</span>
            </button>
          </div>
        </div>

        <!-- 工具 -->
        <div class="relative">
          <button
            class="cursor-pointer rounded-md px-2.5 py-1 text-[11px] transition-colors hover:bg-slate-100 hover:text-slate-900 dark:hover:bg-slate-800/80 dark:hover:text-white"
            :class="activeMenu === 'tools' ? 'bg-slate-200 text-slate-900 dark:bg-slate-800 dark:text-white' : ''"
            @click="activeMenu = activeMenu === 'tools' ? null : 'tools'"
          >
            工具
          </button>
          <div
            v-if="activeMenu === 'tools'"
            class="absolute left-0 top-8 z-50 w-48 animate-in rounded-lg border border-slate-200 bg-white/95 py-1 text-xs shadow-xl backdrop-blur-xl duration-100 dark:border-slate-700/80 dark:bg-slate-900/95"
          >
            <button class="menu-item" @click="exec(() => emit('openSettings'))">
              <span>首选项设置</span><span class="shortcut">Ctrl+,</span>
            </button>
            <button class="menu-item" @click="exec(() => store.toggleTurtleMode())">
              <span>乌龟限速模式 (1MB/s)</span>
              <Check v-if="store.turtleMode" class="h-3.5 w-3.5 text-amber-500" />
            </button>
            <button class="menu-item" @click="exec(() => emit('openLogs'))">
              <span>内核诊断日志</span><span class="shortcut">Ctrl+L</span>
            </button>
          </div>
        </div>

        <!-- 帮助 -->
        <div class="relative">
          <button
            class="cursor-pointer rounded-md px-2.5 py-1 text-[11px] transition-colors hover:bg-slate-100 hover:text-slate-900 dark:hover:bg-slate-800/80 dark:hover:text-white"
            :class="activeMenu === 'help' ? 'bg-slate-200 text-slate-900 dark:bg-slate-800 dark:text-white' : ''"
            @click="activeMenu = activeMenu === 'help' ? null : 'help'"
          >
            帮助
          </button>
          <div
            v-if="activeMenu === 'help'"
            class="absolute left-0 top-8 z-50 w-44 animate-in rounded-lg border border-slate-200 bg-white/95 py-1 text-xs shadow-xl backdrop-blur-xl duration-100 dark:border-slate-700/80 dark:bg-slate-900/95"
          >
            <button class="menu-item" @click="exec(() => emit('openAbout'))">关于 极涌 VortexDL...</button>
          </div>
        </div>
      </nav>
    </div>

    <!-- 中：Spotlight 搜索 -->
    <div class="relative mx-4 w-full max-w-sm">
      <div class="relative flex items-center">
        <Search class="absolute left-3 h-3.5 w-3.5 text-slate-400" />
        <input
          ref="searchInput"
          v-model="searchQuery"
          type="text"
          placeholder="搜索任务、链接或扩展名..."
          class="h-7 w-full rounded-full border border-slate-300/80 bg-slate-100/80 pl-8 pr-16 text-xs text-slate-900 shadow-inner outline-none transition-all placeholder:text-slate-400 hover:bg-slate-100 focus:border-blue-500 focus:bg-white dark:border-slate-700/70 dark:bg-slate-900/80 dark:text-white dark:hover:bg-slate-900 dark:focus:border-blue-500/80"
        />
        <div v-if="searchQuery" class="absolute right-2 flex items-center gap-1.5">
          <span
            v-if="matchCount !== undefined"
            class="rounded bg-slate-200 px-1.5 py-0.5 font-mono text-[10px] text-slate-600 dark:bg-slate-800 dark:text-slate-400"
          >
            {{ matchCount }} 条
          </span>
          <button
            class="cursor-pointer p-0.5 text-slate-400 hover:text-slate-700 dark:hover:text-white"
            @click="searchQuery = ''"
          >
            <X class="h-3 w-3" />
          </button>
        </div>
        <div
          v-else
          class="pointer-events-none absolute right-2 rounded border border-slate-300/80 bg-slate-200/80 px-1.5 py-0.5 font-mono text-[10px] text-slate-500 dark:border-slate-700 dark:bg-slate-800/80"
        >
          Ctrl+F
        </div>
      </div>
    </div>

    <!-- 右：主题 + 窗口控制 -->
    <div class="-mr-3 flex items-center">
      <button
        :title="store.settings.theme === 'dark' ? '切换浅色模式' : '切换深色模式'"
        class="mr-1 cursor-pointer rounded-md p-1.5 text-slate-500 transition-colors hover:bg-slate-100 hover:text-slate-900 dark:text-slate-400 dark:hover:bg-slate-800/60 dark:hover:text-white"
        @click="store.updateSettings({ theme: store.settings.theme === 'dark' ? 'light' : 'dark' })"
      >
        <Sun v-if="store.settings.theme === 'dark'" class="h-3.5 w-3.5" />
        <Moon v-else class="h-3.5 w-3.5" />
      </button>

      <div class="flex items-center">
        <button
          title="最小化"
          class="flex h-10 w-11 items-center justify-center text-slate-600 transition-colors hover:bg-slate-200/80 hover:text-slate-900 dark:text-slate-400 dark:hover:bg-slate-800 dark:hover:text-white"
          :disabled="!isTauri"
          :class="{ 'cursor-not-allowed opacity-40': !isTauri }"
          @click="win?.minimize()"
        >
          <Minus class="h-3.5 w-3.5" />
        </button>
        <button
          title="最大化 / 还原"
          class="flex h-10 w-11 items-center justify-center text-slate-600 transition-colors hover:bg-slate-200/80 hover:text-slate-900 dark:text-slate-400 dark:hover:bg-slate-800 dark:hover:text-white"
          :disabled="!isTauri"
          :class="{ 'cursor-not-allowed opacity-40': !isTauri }"
          @click="win?.toggleMaximize()"
        >
          <Square class="h-3 w-3" />
        </button>
        <button
          title="关闭"
          class="flex h-10 w-11 items-center justify-center text-slate-600 transition-colors hover:bg-[#c42b1c] hover:text-white dark:text-slate-400"
          :disabled="!isTauri"
          :class="{ 'cursor-not-allowed opacity-40': !isTauri }"
          @click="win?.close()"
        >
          <X class="h-4 w-4" />
        </button>
      </div>
    </div>
  </header>
</template>

<style scoped>
.menu-item {
  width: 100%;
  padding: 0.375rem 0.75rem;
  text-align: left;
  display: flex;
  align-items: center;
  justify-content: space-between;
  color: rgb(51 65 85);
  cursor: pointer;
  transition: colors 150ms;
}
:global(.dark) .menu-item {
  color: rgb(203 213 225);
}
.menu-item:hover {
  background: rgb(37 99 235);
  color: #fff;
}
.shortcut {
  font-size: 10px;
  font-family: Consolas, monospace;
  color: rgb(148 163 184);
}
</style>
