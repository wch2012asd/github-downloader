<script setup lang="ts">
// 桌面 Toast（右下角，2.8s 自动消失）
import { AlertCircle, CheckCircle2, Info, X } from 'lucide-vue-next';
import type { ToastMessage } from '../types';

defineProps<{ toasts: ToastMessage[] }>();
const emit = defineEmits<{ (e: 'dismiss', id: string): void }>();
</script>

<template>
  <div
    v-if="toasts.length"
    class="pointer-events-none fixed bottom-4 right-4 z-50 flex select-none flex-col gap-2"
  >
    <div
      v-for="toast in toasts"
      :key="toast.id"
      class="pointer-events-auto flex animate-in items-center gap-2.5 rounded-lg border border-slate-200 bg-white/95 px-3.5 py-2.5 text-xs text-slate-800 shadow-xl backdrop-blur-md duration-200 dark:border-slate-700/60 dark:bg-slate-800/95 dark:text-white"
    >
      <CheckCircle2 v-if="toast.type === 'success'" class="h-4 w-4 shrink-0 text-emerald-500" />
      <AlertCircle v-if="toast.type === 'error'" class="h-4 w-4 shrink-0 text-rose-500" />
      <Info v-if="toast.type === 'info'" class="h-4 w-4 shrink-0 text-blue-500" />
      <span class="font-medium">{{ toast.text }}</span>
      <button
        class="ml-2 cursor-pointer text-slate-400 transition-colors hover:text-slate-700 dark:hover:text-white"
        @click="emit('dismiss', toast.id)"
      >
        <X class="h-3.5 w-3.5" />
      </button>
    </div>
  </div>
</template>
