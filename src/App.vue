<script setup lang="ts">
// 外壳：标题栏 → 流速中枢仪表台 → 任务流列表 → 状态栏 → 弹窗族 → Toast
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { useAppStore } from './stores/app';
import TitleBar from './components/TitleBar.vue';
import FlowSpeedDial from './components/FlowSpeedDial.vue';
import TaskStreamDeck from './components/TaskStreamDeck.vue';
import StatusBar from './components/StatusBar.vue';
import NewTaskModal from './components/NewTaskModal.vue';
import SettingsModal from './components/SettingsModal.vue';
import LogsModal from './components/LogsModal.vue';
import AboutModal from './components/AboutModal.vue';
import ToastContainer from './components/ToastContainer.vue';

const store = useAppStore();
const isNewTaskOpen = ref(false);
const isSettingsOpen = ref(false);
const isLogsOpen = ref(false);
const isAboutOpen = ref(false);

function onKeydown(e: KeyboardEvent) {
  const mod = e.ctrlKey || e.metaKey;
  if (!mod) {
    if (e.key === 'Escape') {
      isNewTaskOpen.value = false;
      isSettingsOpen.value = false;
      isLogsOpen.value = false;
      isAboutOpen.value = false;
    }
    return;
  }
  const k = e.key.toLowerCase();
  if (k === 'n') {
    e.preventDefault();
    isNewTaskOpen.value = true;
  } else if (e.key === ',') {
    e.preventDefault();
    isSettingsOpen.value = true;
  } else if (k === 'r') {
    e.preventDefault();
    store.resumeAll();
  } else if (k === 'p') {
    e.preventDefault();
    store.pauseAll();
  } else if (k === 'l') {
    e.preventDefault();
    isLogsOpen.value = true;
  }
}

onMounted(() => {
  window.addEventListener('keydown', onKeydown);
  // 桌面应用：全局屏蔽 WebView 默认右键菜单（任务卡自带自定义右键菜单）
  window.addEventListener('contextmenu', (e) => e.preventDefault());
  store.init();
});
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown));
</script>

<template>
  <div
    class="flex h-screen w-screen select-none flex-col overflow-hidden bg-[#f3f4f6] font-sans text-slate-900 antialiased transition-colors dark:bg-slate-950 dark:text-slate-100"
  >
    <TitleBar
      v-model:search-query="store.searchQuery"
      :match-count="store.filteredTasks.length"
      @new-task="isNewTaskOpen = true"
      @open-settings="isSettingsOpen = true"
      @open-logs="isLogsOpen = true"
      @open-about="isAboutOpen = true"
      @resume-all="store.resumeAll()"
      @pause-all="store.pauseAll()"
      @clear-completed="store.clearCompleted()"
    />

    <FlowSpeedDial :on-new-task="() => (isNewTaskOpen = true)" />

    <TaskStreamDeck />

    <StatusBar
      :total-tasks="store.tasks.length"
      :active-count="store.aggregateStats.activeCount"
      :total-speed="store.aggregateStats.totalSpeed"
      :total-upload-speed="store.aggregateStats.totalUploadSpeed"
      :dht-nodes="store.dhtNodes ?? undefined"
    />

    <NewTaskModal :is-open="isNewTaskOpen" @close="isNewTaskOpen = false" />
    <SettingsModal :is-open="isSettingsOpen" @close="isSettingsOpen = false" />
    <LogsModal :is-open="isLogsOpen" @close="isLogsOpen = false" @clear-logs="store.logs = []" />
    <AboutModal :is-open="isAboutOpen" @close="isAboutOpen = false" />

    <ToastContainer :toasts="store.toasts" @dismiss="store.dismissToast" />
  </div>
</template>
