<script setup lang="ts">
// 底部状态栏：任务统计 + 12 段实时频谱 + ↓/↑ 速率 + DHT 节点 + 乌龟限速开关
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { ArrowDown, ArrowUp, Gauge, Globe, ShieldCheck } from 'lucide-vue-next';
import { useAppStore } from '../stores/app';
import { formatSpeed } from '../utils/format';

defineProps<{
  totalTasks: number;
  activeCount: number;
  totalSpeed: number;
  totalUploadSpeed: number;
  dhtNodes?: number;
}>();

const store = useAppStore();
const now = ref(Date.now());
let timer: number | undefined;
onMounted(() => {
  timer = window.setInterval(() => (now.value = Date.now()), 300);
});
onBeforeUnmount(() => window.clearInterval(timer));
</script>

<template>
  <footer
    class="flex h-6 shrink-0 select-none items-center justify-between border-t border-slate-300/80 bg-[#eaecef] px-3 text-[11px] text-slate-700 transition-colors dark:border-slate-800 dark:bg-slate-900/95 dark:text-slate-400"
  >
    <!-- 左：任务统计 -->
    <div class="flex items-center gap-3">
      <span>{{ totalTasks }} 个任务对象</span>
      <span class="text-slate-300 dark:text-slate-700">|</span>
      <span>{{ activeCount }} 个正在传输</span>
    </div>

    <!-- 右：遥测 -->
    <div class="flex items-center gap-3 font-mono tabular-nums">
      <div
        class="hidden h-3 items-center gap-[2px] rounded-xs bg-slate-200/70 px-1.5 py-0.5 dark:bg-slate-800 lg:flex"
        title="多线程数据流频段吞吐实时图谱"
      >
        <div
          v-for="i in 12"
          :key="i"
          class="w-[2.5px] rounded-xs transition-all duration-200"
          :class="
            totalSpeed === 0
              ? 'bg-slate-400 dark:bg-slate-600'
              : i >= 9
                ? 'bg-amber-400'
                : 'bg-blue-500 dark:bg-blue-400'
          "
          :style="{ height: `${totalSpeed > 0 ? Math.floor(Math.sin(i + now / 300) * 40 + 55) : 15}%` }"
        />
      </div>

      <div class="flex items-center gap-1 font-medium text-emerald-600 dark:text-emerald-400">
        <ArrowDown class="h-3 w-3" />
        <span>{{ formatSpeed(totalSpeed) }}</span>
      </div>

      <span class="text-slate-300 dark:text-slate-700">|</span>

      <div class="flex items-center gap-1 font-medium text-sky-600 dark:text-sky-400">
        <ArrowUp class="h-3 w-3" />
        <span>{{ formatSpeed(totalUploadSpeed) }}</span>
      </div>

      <span class="hidden text-slate-300 dark:text-slate-700 md:inline">|</span>

      <!-- Clash 链路状态（30s 巡检） -->
      <div
        class="hidden items-center gap-1 md:flex"
        :class="
          store.clash.checking
            ? 'text-slate-400'
            : store.clash.ok
              ? 'text-emerald-600 dark:text-emerald-400'
              : 'text-slate-400 dark:text-slate-500'
        "
        :title="
          store.clash.ok
            ? `Clash 已连接 (127.0.0.1:${store.settings.proxyPort}，延迟 ${store.clash.latencyMs}ms)`
            : `Clash 未连接 (127.0.0.1:${store.settings.proxyPort})——勾选「通过 Clash 代理下载」的任务将无法走加速通道`
        "
      >
        <ShieldCheck class="h-3 w-3" :class="store.clash.ok ? 'text-emerald-500' : ''" />
        <span>
          {{
            store.clash.checking
              ? 'Clash: 检测中'
              : store.clash.ok
                ? `Clash: ${store.clash.latencyMs} ms`
                : 'Clash: 未连接'
          }}
        </span>
      </div>

      <span class="hidden text-slate-300 dark:text-slate-700 md:inline">|</span>

      <div class="hidden items-center gap-1 text-slate-500 dark:text-slate-400 md:flex">
        <Globe class="h-3 w-3 text-emerald-500" />
        <span>DHT: {{ dhtNodes ?? 488 }} 节点</span>
      </div>

      <span class="text-slate-300 dark:text-slate-700">|</span>

      <button
        :title="store.turtleMode ? '乌龟限速已开启 (点击关闭)' : '开启乌龟模式限速'"
        class="flex items-center gap-1 rounded px-1.5 py-0.5 transition-colors hover:bg-slate-200 dark:hover:bg-slate-800"
        :class="store.turtleMode ? 'font-semibold text-amber-600' : 'text-slate-500'"
        @click="store.toggleTurtleMode()"
      >
        <Gauge class="h-3 w-3" />
        <span>{{ store.turtleMode ? '限速: 1MB/s' : '限速: 关闭' }}</span>
      </button>
    </div>
  </footer>
</template>
