<script setup lang="ts">
// 关于：真实技术栈信息（不沿用原型的虚构 Aria2c 文案）
import { Cpu, Globe, Layers, Sparkles, X, Zap } from 'lucide-vue-next';

defineProps<{ isOpen: boolean }>();
const emit = defineEmits<{ (e: 'close'): void }>();

const version = __APP_VERSION__;
</script>

<template>
  <div
    v-if="isOpen"
    class="fixed inset-0 z-50 flex select-none items-center justify-center bg-slate-900/60 p-4 backdrop-blur-xs"
    @mousedown.self="emit('close')"
  >
    <div
      class="flex max-h-[90vh] w-full max-w-lg animate-in flex-col overflow-hidden rounded-2xl border border-slate-200 bg-white shadow-2xl transition-all duration-200 dark:border-slate-800 dark:bg-slate-900"
    >
      <div
        class="relative flex flex-col items-center border-b border-slate-100 bg-gradient-to-b from-blue-50/60 to-transparent p-6 text-center dark:border-slate-800/80 dark:from-blue-950/20"
      >
        <button
          class="absolute right-4 top-4 cursor-pointer rounded-md p-1.5 text-slate-400 transition-colors hover:bg-slate-100 hover:text-slate-700 dark:hover:bg-slate-800 dark:hover:text-slate-200"
          @click="emit('close')"
        >
          <X class="h-4 w-4" />
        </button>

        <div class="group relative mb-3">
          <div class="h-20 w-20 overflow-hidden rounded-xl shadow-xl ring-1 ring-slate-900/10 transition-transform duration-300 group-hover:scale-105 dark:ring-white/10">
            <img src="/vortex-icon.png" alt="极涌 VortexDL 图标" class="h-full w-full object-cover" />
          </div>
          <div class="absolute -bottom-1 -right-1 rounded-md bg-blue-600 p-1 text-white shadow-md">
            <Zap class="h-3 w-3 fill-current" />
          </div>
        </div>

        <h2 class="flex items-center gap-2 text-lg font-bold text-slate-900 dark:text-slate-100">
          <span>极涌 VortexDL</span>
          <span class="rounded bg-blue-100 px-2 py-0.5 font-mono text-xs font-medium text-blue-700 dark:bg-blue-950 dark:text-blue-300">
            v{{ version }} (x64)
          </span>
        </h2>
        <p class="mt-1 max-w-sm text-xs text-slate-500 dark:text-slate-400">
          极速多线程与分布式下载加速引擎 · 专为 Windows 11 / 10 打造
        </p>
      </div>

      <div class="max-h-[420px] space-y-5 overflow-y-auto p-6 text-xs">
        <div class="space-y-3 rounded-lg border border-slate-200/80 bg-slate-50 p-4 dark:border-slate-800 dark:bg-slate-800/50">
          <div class="flex items-center gap-2 font-semibold text-slate-800 dark:text-slate-200">
            <Sparkles class="h-4 w-4 text-blue-600 dark:text-blue-400" />
            <span>设计理念与技术架构</span>
          </div>
          <div class="space-y-2 text-[11px] leading-relaxed text-slate-600 dark:text-slate-400">
            <div>
              <strong class="text-slate-800 dark:text-slate-200">多通道引擎：</strong>
              Rust 原生内核（VortexCore）——HTTP(S) 多连接分片 + 多源镜像聚合动态调度 + BT/磁力（librqbit，DHT/UPnP），断点续传按源校验 ETag，30 秒失速自动判中断重试。
            </div>
            <div>
              <strong class="text-slate-800 dark:text-slate-200">GitHub 加速通道：</strong>
              Clash 本地代理、GHProxy、KKGitHub、Moeyy 镜像与官方直连五通道自动测速切换，许可门槛站点自动确认。
            </div>
            <div>
              <strong class="text-slate-800 dark:text-slate-200">桌面原生体验：</strong>
              Windows 11 Fluent 视觉、无边框自绘标题栏、全局搜索（Ctrl+F）、任务流卡片与分段式多线程进度可视化。
            </div>
          </div>
        </div>

        <div class="grid grid-cols-3 gap-2.5 text-center">
          <div class="rounded-lg border border-slate-200 bg-white p-2.5 dark:border-slate-800 dark:bg-slate-900">
            <div class="mb-1 flex items-center justify-center text-blue-600 dark:text-blue-400"><Cpu class="h-4 w-4" /></div>
            <div class="text-[10px] text-slate-400">内核引擎</div>
            <div class="mt-0.5 font-mono font-semibold text-slate-800 dark:text-slate-200">VortexCore (Rust)</div>
          </div>
          <div class="rounded-lg border border-slate-200 bg-white p-2.5 dark:border-slate-800 dark:bg-slate-900">
            <div class="mb-1 flex items-center justify-center text-teal-600 dark:text-teal-400"><Layers class="h-4 w-4" /></div>
            <div class="text-[10px] text-slate-400">并发线程上限</div>
            <div class="mt-0.5 font-mono font-semibold text-slate-800 dark:text-slate-200">32 线程动态切片</div>
          </div>
          <div class="rounded-lg border border-slate-200 bg-white p-2.5 dark:border-slate-800 dark:bg-slate-900">
            <div class="mb-1 flex items-center justify-center text-emerald-600 dark:text-emerald-400"><Globe class="h-4 w-4" /></div>
            <div class="text-[10px] text-slate-400">网络协议</div>
            <div class="mt-0.5 font-mono font-semibold text-slate-800 dark:text-slate-200">HTTP · BT · DHT</div>
          </div>
        </div>
      </div>

      <div
        class="flex items-center justify-between border-t border-slate-100 bg-slate-50/50 p-4 dark:border-slate-800 dark:bg-slate-900/50"
      >
        <span class="text-[11px] text-slate-400">Designed for Desktop · 2026</span>
        <button
          class="cursor-pointer rounded-md bg-blue-600 px-4 py-1.5 text-xs font-semibold text-white shadow-xs transition-colors hover:bg-blue-700"
          @click="emit('close')"
        >
          我知道了
        </button>
      </div>
    </div>
  </div>
</template>
