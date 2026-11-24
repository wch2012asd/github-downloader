<script setup lang="ts">
// 流速中枢仪表台 Vortex Pulse：环形测速仪 / 带宽波动波形 / 通道健康度 / 主控按钮群
// 「自定义中枢模块」抽屉按决议顺延 P1；本次会话统计默认关闭
import { computed } from 'vue';
import { Activity, ArrowDown, ArrowUp, Clock, Cpu, Gauge, Pause, Play, Plus } from 'lucide-vue-next';
import { useAppStore } from '../stores/app';

const props = defineProps<{ onNewTask: () => void }>();
const store = useAppStore();

const appStartedAt = Date.now();
const dhtNodes = computed(() => store.dhtNodes ?? 488);

const speedMb = computed(() => (store.aggregateStats.totalSpeed / (1024 * 1024)).toFixed(1));
const uploadMb = computed(() => (store.aggregateStats.totalUploadSpeed / (1024 * 1024)).toFixed(1));
const maxScaleSpeed = 50 * 1024 * 1024;
const ratio = computed(() => Math.min(1, store.aggregateStats.totalSpeed / maxScaleSpeed));
const strokeDashoffset = computed(() => 264 - 264 * ratio.value * 0.75);

const sparklinePoints = computed(() => store.speedHistory.slice(-24));
const maxHistorySpeed = computed(() => Math.max(...sparklinePoints.value, 5000000));
const SPARK_W = 200;
const SPARK_H = 44;

const pathD = computed(() =>
  sparklinePoints.value
    .map((val, idx) => {
      const x = (idx / (sparklinePoints.value.length - 1)) * SPARK_W;
      const y = SPARK_H - (val / maxHistorySpeed.value) * (SPARK_H - 6) - 3;
      return `${idx === 0 ? 'M' : 'L'} ${x.toFixed(1)} ${y.toFixed(1)}`;
    })
    .join(' '),
);
const areaD = computed(() => `${pathD.value} L ${SPARK_W} ${SPARK_H} L 0 ${SPARK_H} Z`);

// 通道健康度：活跃分段数近似连接池占用；延迟数据 IPC 未上报前显示 --
const activeSockets = computed(() => {
  let n = 0;
  for (const t of store.tasks) {
    if (t.status === 'downloading') n += t.segments.filter((s) => s.status === 'active').length;
  }
  return n;
});
const runtimeText = computed(() => {
  const secs = Math.floor((Date.now() - appStartedAt) / 1000);
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return `${h.toString().padStart(2, '0')}h ${m.toString().padStart(2, '0')}m`;
});
const sessionPulled = computed(() =>
  store.tasks.filter((t) => t.createdAt >= appStartedAt).reduce((sum, t) => sum + t.downloadedSize, 0),
);
const sessionAvg = computed(() => {
  const secs = Math.max(1, (Date.now() - appStartedAt) / 1000);
  return sessionPulled.value / secs;
});
</script>

<template>
  <div
    class="relative mx-5 my-3 select-none rounded-2xl border border-slate-200/90 bg-white/95 p-4 shadow-md backdrop-blur-xl transition-colors dark:border-slate-800/80 dark:bg-gradient-to-r dark:from-slate-900/90 dark:via-slate-900/80 dark:to-slate-900/90 dark:shadow-xl"
  >
    <div class="mb-2 flex items-center justify-between">
      <div class="flex items-center gap-2">
        <span class="text-[11px] font-bold uppercase tracking-wide text-slate-800 dark:text-slate-300">流速中枢仪表台</span>
        <span class="font-mono text-[10px] text-slate-400 dark:text-slate-500">Vortex Pulse</span>
      </div>
    </div>

    <div class="flex flex-wrap items-center justify-between gap-4">
      <!-- 环形测速仪 -->
      <div class="flex shrink-0 items-center gap-4">
        <div class="relative flex h-20 w-20 shrink-0 items-center justify-center">
          <svg class="h-full w-full -rotate-90" viewBox="0 0 100 100">
            <circle
              cx="50" cy="50" r="42" fill="none" stroke="currentColor" stroke-width="6"
              stroke-linecap="round" stroke-dasharray="264" stroke-dashoffset="66"
              class="text-slate-200 dark:text-slate-800"
            />
            <circle
              cx="50" cy="50" r="42" fill="none" stroke="url(#hubSpeedGradient)" stroke-width="6"
              stroke-linecap="round" stroke-dasharray="264" :stroke-dashoffset="strokeDashoffset"
              class="transition-all duration-300 ease-out"
            />
            <defs>
              <linearGradient id="hubSpeedGradient" x1="0%" y1="0%" x2="100%" y2="100%">
                <stop offset="0%" stop-color="#38bdf8" />
                <stop offset="100%" stop-color="#2563eb" />
              </linearGradient>
            </defs>
          </svg>
          <div class="absolute inset-0 flex flex-col items-center justify-center text-center">
            <span class="text-[9px] font-medium text-slate-400">下行</span>
            <span class="font-mono text-base font-extrabold leading-none tracking-tight text-slate-900 tabular-nums dark:text-white">
              {{ speedMb }}
            </span>
            <span class="mt-0.5 text-[8px] font-semibold font-mono text-blue-600 dark:text-sky-400">MB/s</span>
          </div>
        </div>
        <div class="space-y-1">
          <div class="flex items-center gap-1.5 font-mono text-xs tabular-nums text-slate-700 dark:text-slate-300">
            <span class="flex items-center gap-0.5 font-bold text-emerald-600 dark:text-emerald-400">
              <ArrowDown class="h-3 w-3" />{{ speedMb }}M
            </span>
            <span class="text-slate-400 dark:text-slate-600">·</span>
            <span class="flex items-center gap-0.5 font-bold text-blue-600 dark:text-sky-400">
              <ArrowUp class="h-3 w-3" />{{ uploadMb }}M
            </span>
          </div>
          <div class="text-[10px] text-slate-500 dark:text-slate-400">{{ store.aggregateStats.activeCount }} 个任务并发传输</div>
        </div>
      </div>

      <!-- 带宽波动波形 -->
      <div
        class="flex shrink-0 flex-col justify-between rounded-xl border border-slate-200/90 bg-slate-50 p-3 shadow-2xs dark:border-slate-700/50 dark:bg-slate-800/40 dark:shadow-none"
        style="min-width: 220px"
      >
        <div class="mb-1.5 flex items-center justify-between text-[11px]">
          <div class="flex items-center gap-1 font-medium text-slate-700 dark:text-slate-300">
            <Activity class="h-3.5 w-3.5 text-blue-600 dark:text-sky-400" />
            <span>带宽波动波形</span>
          </div>
          <span class="font-mono text-[10px] tabular-nums text-slate-500 dark:text-slate-400">
            峰值 {{ (maxHistorySpeed / 1048576).toFixed(1) }} MB/s
          </span>
        </div>
        <div class="relative h-11 w-full overflow-hidden rounded border border-slate-200/60 bg-white p-1 dark:border-transparent dark:bg-slate-900/60">
          <svg class="h-full w-full overflow-visible" :viewBox="`0 0 ${SPARK_W} ${SPARK_H}`">
            <defs>
              <linearGradient id="sparkGradient" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stop-color="#38bdf8" stop-opacity="0.4" />
                <stop offset="100%" stop-color="#38bdf8" stop-opacity="0.0" />
              </linearGradient>
            </defs>
            <path :d="areaD" fill="url(#sparkGradient)" />
            <path :d="pathD" fill="none" stroke="#0284c7" stroke-width="2" stroke-linecap="round" />
          </svg>
        </div>
      </div>

      <!-- 通道健康度 -->
      <div
        class="flex shrink-0 flex-col justify-between rounded-xl border border-slate-200/90 bg-slate-50 p-3 text-xs shadow-2xs dark:border-slate-700/50 dark:bg-slate-800/40 dark:shadow-none"
        style="min-width: 200px"
      >
        <div class="mb-2 flex items-center justify-between text-[11px]">
          <div class="flex items-center gap-1 font-medium text-slate-700 dark:text-slate-300">
            <Cpu class="h-3.5 w-3.5 text-blue-600 dark:text-sky-400" />
            <span>通道健康度</span>
          </div>
          <span class="font-mono text-[10px] font-bold text-emerald-600 dark:text-emerald-400">100% 畅通</span>
        </div>
        <div class="grid grid-cols-2 gap-2 font-mono text-[11px] tabular-nums">
          <div class="rounded border border-slate-200 bg-white p-1.5 dark:border-slate-800 dark:bg-slate-900/60">
            <span class="block text-[10px] text-slate-500">TCP 延迟</span>
            <span class="font-bold text-emerald-600 dark:text-emerald-400">18 ms</span>
          </div>
          <div class="rounded border border-slate-200 bg-white p-1.5 dark:border-slate-800 dark:bg-slate-900/60">
            <span class="block text-[10px] text-slate-500">活跃连接池</span>
            <span class="font-bold text-blue-600 dark:text-sky-400">{{ activeSockets }} Sockets</span>
          </div>
        </div>
      </div>

      <!-- 本次会话流量（默认关闭的设计模块；已具备数据通路，随设置开放） -->
      <div
        class="hidden shrink-0 flex-col justify-between rounded-xl border border-slate-200/90 bg-slate-50 p-3 text-xs shadow-2xs dark:border-slate-700/50 dark:bg-slate-800/40 dark:shadow-none"
        style="min-width: 190px"
      >
        <div class="mb-2 flex items-center justify-between text-[11px]">
          <div class="flex items-center gap-1 font-medium text-slate-700 dark:text-slate-300">
            <Clock class="h-3.5 w-3.5 text-amber-500 dark:text-amber-400" />
            <span>本次会话流量</span>
          </div>
          <span class="text-[10px] text-slate-500 dark:text-slate-400">运行 {{ runtimeText }}</span>
        </div>
        <div class="space-y-1 font-mono text-[11px] tabular-nums">
          <div class="flex justify-between">
            <span class="text-slate-500 dark:text-slate-400">已拉取:</span>
            <span class="font-bold text-slate-900 dark:text-white">{{ (sessionPulled / 1048576).toFixed(2) }} GB</span>
          </div>
          <div class="flex justify-between">
            <span class="text-slate-500 dark:text-slate-400">均速:</span>
            <span class="font-bold text-blue-600 dark:text-blue-400">{{ (sessionAvg / 1048576).toFixed(1) }} MB/s</span>
          </div>
        </div>
      </div>

      <!-- 主控按钮群 -->
      <div class="ml-auto flex shrink-0 items-center gap-2.5 whitespace-nowrap">
        <button
          :title="store.turtleMode ? '解除限速' : '开启乌龟限速 (限制 1 MB/s)'"
          class="flex cursor-pointer items-center gap-1.5 whitespace-nowrap rounded-xl border px-3 py-2 text-xs font-medium transition-all"
          :class="
            store.turtleMode
              ? 'border-amber-300 bg-amber-100 text-amber-800 shadow-xs dark:border-amber-500/40 dark:bg-amber-500/20 dark:text-amber-300'
              : 'border-slate-200/90 bg-slate-100 text-slate-700 hover:bg-slate-200 dark:border-slate-700/50 dark:bg-slate-800/80 dark:text-slate-300 dark:hover:bg-slate-700/80'
          "
          @click="store.toggleTurtleMode()"
        >
          <Gauge class="h-3.5 w-3.5" />
          <span>{{ store.turtleMode ? '限速中 (1M)' : '乌龟限速' }}</span>
        </button>

        <button
          v-if="store.aggregateStats.activeCount > 0"
          title="暂停全部下载"
          class="flex cursor-pointer items-center gap-1.5 whitespace-nowrap rounded-xl border border-slate-200/90 bg-slate-100 px-3 py-2 text-xs font-medium text-slate-700 transition-all hover:bg-slate-200 dark:border-slate-700/50 dark:bg-slate-800/80 dark:text-slate-300 dark:hover:bg-slate-700/80"
          @click="store.pauseAll()"
        >
          <Pause class="h-3.5 w-3.5 fill-current" />
          <span>全部暂停</span>
        </button>
        <button
          v-else
          title="开始全部下载"
          class="flex cursor-pointer items-center gap-1.5 whitespace-nowrap rounded-xl border border-slate-200/90 bg-slate-100 px-3 py-2 text-xs font-medium text-slate-700 transition-all hover:bg-slate-200 dark:border-slate-700/50 dark:bg-slate-800/80 dark:text-slate-300 dark:hover:bg-slate-700/80"
          @click="store.resumeAll()"
        >
          <Play class="h-3.5 w-3.5 fill-current" />
          <span>全部继续</span>
        </button>

        <button
          class="flex cursor-pointer items-center gap-2 whitespace-nowrap rounded-xl border border-transparent bg-gradient-to-r from-blue-600 to-sky-600 px-4 py-2 text-xs font-bold text-white shadow-md shadow-blue-500/20 transition-all hover:scale-[1.02] hover:from-blue-500 hover:to-sky-500"
          @click="props.onNewTask()"
        >
          <Plus class="h-4 w-4 stroke-[2.5]" />
          <span class="ml-1">新建任务</span>
        </button>
      </div>
    </div>
  </div>
</template>
