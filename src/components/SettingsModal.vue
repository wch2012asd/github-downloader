<script setup lang="ts">
// 首选项设置：常规 / 网络限速 / BT / 外观 四标签；本地副本编辑 + 保存提交
import { reactive, ref, watch } from 'vue';
import { Folder, Gauge, HardDrive, Palette, Settings as SettingsIcon, Share2, X } from 'lucide-vue-next';
import { useAppStore } from '../stores/app';
import { api, isTauri } from '../lib/bridge';
import type { AppSettings } from '../types';

const props = defineProps<{ isOpen: boolean }>();
const emit = defineEmits<{ (e: 'close'): void }>();

const store = useAppStore();
const activeTab = ref<'general' | 'network' | 'torrent' | 'appearance'>('general');
const local = reactive<AppSettings>({ ...store.settings });
const savedSuccess = ref(false);
const detecting = ref(false);
const testing = ref(false);
const clashOk = ref(false);
const clashTestText = ref('');

async function detectPort() {
  if (!isTauri) {
    clashTestText.value = '浏览器演示模式不可用';
    clashOk.value = false;
    return;
  }
  detecting.value = true;
  try {
    const port = await api.detectClashPort();
    if (port) {
      local.proxyPort = port;
      clashOk.value = true;
      clashTestText.value = `已检测到 Clash 端口 ${port}`;
      store.addLog(`自动检测到 Clash 本地代理端口: ${port}`, 'info');
    } else {
      clashOk.value = false;
      clashTestText.value = '未检测到运行中的 Clash';
      store.addLog('未检测到运行中的 Clash（注册表与常见端口均未探活）', 'warn');
    }
  } finally {
    detecting.value = false;
  }
}

async function testConnection() {
  if (!isTauri) {
    clashTestText.value = '浏览器演示模式不可用';
    clashOk.value = false;
    return;
  }
  testing.value = true;
  try {
    const r = await api.testClashProxy(local.proxyPort);
    if (r.ok) {
      clashOk.value = true;
      clashTestText.value = `连接正常，延迟 ${r.latency_ms} ms`;
      store.addLog(`Clash 连接测试通过: 127.0.0.1:${local.proxyPort} (${r.latency_ms}ms)`, 'info');
    } else if (!r.tcp_open) {
      clashOk.value = false;
      clashTestText.value = `${local.proxyPort} 端口无响应（Clash 未运行或端口不对）`;
      store.addLog(`Clash 连接测试失败: 端口 ${local.proxyPort} 无响应`, 'warn');
    } else {
      clashOk.value = false;
      clashTestText.value = '端口已连通，但经代理的测试请求失败（节点波动或 GitHub 链路不稳，可稍后重试）';
      store.addLog(`Clash 端口 ${local.proxyPort} 可达，但经代理的测试请求失败`, 'warn');
    }
  } finally {
    testing.value = false;
  }
}

watch(
  () => props.isOpen,
  (open) => {
    if (open) Object.assign(local, store.settings);
  },
);

function save() {
  store.updateSettings({ ...local });
  savedSuccess.value = true;
  setTimeout(() => {
    savedSuccess.value = false;
    emit('close');
  }, 400);
}

async function browseFolder() {
  if (!isTauri) return;
  try {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const picked = await open({ directory: true, multiple: false, title: '选择默认下载目录' });
    if (typeof picked === 'string' && picked) local.defaultSavePath = picked;
  } catch (e) {
    store.addToast(`目录选择失败: ${String(e)}`, 'error');
  }
}

function copyToken(token: string) {
  if (!token) return;
  navigator.clipboard.writeText(token);
  store.addToast('接管 Token 已复制', 'info');
}
</script>

<template>
  <div
    v-if="isOpen"
    class="fixed inset-0 z-50 flex select-none items-center justify-center bg-slate-900/50 p-4 backdrop-blur-xs"
    @mousedown.self="emit('close')"
  >
    <div
      class="flex max-h-[90vh] w-full max-w-xl flex-col overflow-hidden rounded-xl border border-slate-200 bg-white shadow-2xl transition-all dark:border-slate-800 dark:bg-slate-900"
    >
      <div class="flex items-center justify-between border-b border-slate-200 px-5 py-3.5 dark:border-slate-800">
        <h2 class="flex items-center gap-2 text-sm font-bold text-slate-900 dark:text-slate-100">
          <SettingsIcon class="h-4 w-4 text-blue-600" />
          <span>首选项与设置</span>
        </h2>
        <button
          class="cursor-pointer rounded-md p-1 text-slate-400 hover:text-slate-700 dark:hover:text-slate-200"
          @click="emit('close')"
        >
          <X class="h-4 w-4" />
        </button>
      </div>

      <div class="flex min-h-[380px] flex-1">
        <div class="w-44 space-y-1 border-r border-slate-200 bg-slate-50 p-2.5 dark:border-slate-800 dark:bg-slate-900/50">
          <button
            v-for="t in [
              { id: 'general', label: '常规基础', icon: HardDrive },
              { id: 'network', label: '网络与限速', icon: Gauge },
              { id: 'torrent', label: 'BitTorrent / P2P', icon: Share2 },
              { id: 'appearance', label: '外观与风格', icon: Palette },
            ]"
            :key="t.id"
            class="flex w-full items-center gap-2 rounded-md px-3 py-2 text-xs font-medium transition-colors"
            :class="
              activeTab === t.id
                ? 'bg-white text-slate-900 shadow-xs dark:bg-slate-800 dark:text-slate-100'
                : 'text-slate-600 hover:bg-slate-100 dark:text-slate-400 dark:hover:bg-slate-800/60'
            "
            @click="activeTab = t.id as typeof activeTab"
          >
            <component :is="t.icon" class="h-4 w-4 text-slate-400" />
            <span>{{ t.label }}</span>
          </button>
        </div>

        <div class="flex-1 space-y-4 overflow-y-auto p-5 text-xs">
          <!-- 常规 -->
          <div v-if="activeTab === 'general'" class="space-y-4">
            <div class="space-y-1.5">
              <label class="font-semibold text-slate-700 dark:text-slate-300">默认下载保存路径</label>
              <div class="flex items-center gap-2">
                <input
                  v-model="local.defaultSavePath"
                  type="text"
                  class="h-9 flex-1 rounded-md border border-slate-200 bg-slate-50 px-3 font-mono text-xs text-slate-900 outline-none focus:border-blue-500 dark:border-slate-700 dark:bg-slate-800/60 dark:text-slate-100"
                />
                <button
                  type="button"
                  title="选择默认下载目录"
                  class="flex h-9 items-center gap-1.5 rounded-md border border-slate-200 bg-slate-100 px-3 text-slate-700 transition-colors hover:bg-slate-200 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300 dark:hover:bg-slate-700"
                  @click="browseFolder"
                >
                  <Folder class="h-3.5 w-3.5" /><span>浏览</span>
                </button>
              </div>
            </div>
            <div class="space-y-1.5">
              <div class="flex items-center justify-between">
                <label class="font-semibold text-slate-700 dark:text-slate-300">默认分块线程数（新建任务）</label>
                <span class="font-mono font-semibold text-blue-600 dark:text-blue-400">{{ local.defaultThreads }} 线程</span>
              </div>
              <input v-model.number="local.defaultThreads" type="range" min="1" max="32" class="w-full cursor-pointer accent-blue-600" />
            </div>
            <div class="space-y-1.5">
              <label class="font-semibold text-slate-700 dark:text-slate-300">同时下载任务数上限</label>
              <input
                v-model.number="local.maxConcurrent"
                type="number"
                min="1"
                max="10"
                class="h-9 w-full rounded-md border border-slate-200 bg-slate-50 px-3 font-mono text-xs text-slate-900 outline-none focus:border-blue-500 dark:border-slate-700 dark:bg-slate-800/60 dark:text-slate-100"
              />
            </div>
            <label class="flex cursor-pointer items-center gap-2">
              <input v-model="local.soundEnabled" type="checkbox" class="h-4 w-4 rounded text-blue-600 focus:ring-0" />
              <span class="text-slate-700 dark:text-slate-300">下载完成提示音</span>
            </label>
          </div>

          <!-- 网络与限速 -->
          <div v-else-if="activeTab === 'network'" class="space-y-4">
            <div class="space-y-1.5">
              <div class="flex items-center justify-between">
                <label class="font-semibold text-slate-700 dark:text-slate-300">Clash 本地代理端口</label>
                <span
                  v-if="clashTestText"
                  class="font-mono text-[11px]"
                  :class="clashOk ? 'text-emerald-600 dark:text-emerald-400' : 'text-rose-500'"
                >
                  {{ clashTestText }}
                </span>
              </div>
              <input
                v-model.number="local.proxyPort"
                type="number"
                min="1024"
                max="65535"
                class="h-9 w-full rounded-md border border-slate-200 bg-slate-50 px-3 font-mono text-xs text-slate-900 outline-none focus:border-blue-500 dark:border-slate-700 dark:bg-slate-800/60 dark:text-slate-100"
              />
              <p class="text-[10px] text-slate-400">勾选「通过 Clash 代理下载」的任务将经 127.0.0.1:此端口 转发</p>
              <div class="flex items-center gap-2 pt-1">
                <button
                  type="button"
                  :disabled="detecting || testing"
                  class="cursor-pointer rounded-md border border-slate-200 bg-slate-100 px-2.5 py-1 text-[11px] text-slate-700 transition-colors hover:bg-slate-200 disabled:cursor-not-allowed disabled:opacity-50 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300 dark:hover:bg-slate-700"
                  @click="detectPort"
                >
                  {{ detecting ? '检测中…' : '自动检测端口' }}
                </button>
                <button
                  type="button"
                  :disabled="detecting || testing"
                  class="cursor-pointer rounded-md border border-slate-200 bg-slate-100 px-2.5 py-1 text-[11px] text-slate-700 transition-colors hover:bg-slate-200 disabled:cursor-not-allowed disabled:opacity-50 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300 dark:hover:bg-slate-700"
                  @click="testConnection"
                >
                  {{ testing ? '测试中…' : '测试连接' }}
                </button>
              </div>
            </div>
            <div class="space-y-1.5">
              <div class="flex items-center justify-between">
                <label class="font-semibold text-slate-700 dark:text-slate-300">全局下载限速</label>
                <span class="font-mono font-semibold text-blue-600 dark:text-blue-400">
                  {{ local.globalDownloadLimit === 0 ? '不限速' : local.globalDownloadLimit + ' KB/s' }}
                </span>
              </div>
              <input v-model.number="local.globalDownloadLimit" type="range" min="0" max="51200" step="1024" class="w-full cursor-pointer accent-blue-600" />
              <p class="text-[10px] text-slate-400">0 = 不限速；乌龟限速开关等效于 1024 KB/s 全局限速</p>
            </div>
            <div class="space-y-1.5">
              <label class="font-semibold text-slate-700 dark:text-slate-300">全局上传限速（BT 做种）</label>
              <input
                v-model.number="local.globalUploadLimit"
                type="number"
                min="0"
                class="h-9 w-full rounded-md border border-slate-200 bg-slate-50 px-3 font-mono text-xs text-slate-900 outline-none focus:border-blue-500 dark:border-slate-700 dark:bg-slate-800/60 dark:text-slate-100"
              />
              <p class="text-[10px] text-slate-400">0 = 不限；仅影响 BT 任务上传通道</p>
            </div>
          </div>

          <!-- BT -->
          <div v-else-if="activeTab === 'torrent'" class="space-y-4">
            <label class="flex cursor-pointer items-center gap-2">
              <input v-model="local.dhtEnabled" type="checkbox" class="h-4 w-4 rounded text-blue-600 focus:ring-0" />
              <span class="text-slate-700 dark:text-slate-300">启用 DHT 分布式网络（磁力链接依赖）</span>
            </label>
            <div class="space-y-1.5">
              <label class="font-semibold text-slate-700 dark:text-slate-300">BT 监听端口</label>
              <input
                v-model.number="local.btPort"
                type="number"
                min="1024"
                max="65535"
                class="h-9 w-full rounded-md border border-slate-200 bg-slate-50 px-3 font-mono text-xs text-slate-900 outline-none focus:border-blue-500 dark:border-slate-700 dark:bg-slate-800/60 dark:text-slate-100"
              />
            </div>

            <div class="space-y-2 rounded-lg border border-slate-200 bg-slate-50 p-3 dark:border-slate-700 dark:bg-slate-800/40">
              <label class="flex cursor-pointer items-center gap-2">
                <input v-model="local.browserTakeoverEnabled" type="checkbox" class="h-4 w-4 rounded text-blue-600 focus:ring-0" />
                <span class="font-semibold text-slate-700 dark:text-slate-300">浏览器下载接管（本机 127.0.0.1:{{ local.browserTakeoverPort }}）</span>
              </label>
              <div class="space-y-1.5">
                <label class="text-[11px] font-semibold text-slate-600 dark:text-slate-400">接管监听端口（改后需重启应用）</label>
                <input
                  v-model.number="local.browserTakeoverPort"
                  type="number"
                  min="1024"
                  max="65535"
                  class="h-8 w-full rounded-md border border-slate-200 bg-slate-50 px-2.5 font-mono text-[11px] text-slate-900 outline-none focus:border-blue-500 dark:border-slate-700 dark:bg-slate-800/60 dark:text-slate-100"
                />
              </div>
              <div class="space-y-1.5">
                <label class="text-[11px] font-semibold text-slate-600 dark:text-slate-400">接管 Token（浏览器扩展选项页填入）</label>
                <div class="flex items-center gap-2">
                  <input
                    :value="local.browserTakeoverToken"
                    type="text"
                    readonly
                    class="h-8 flex-1 rounded-md border border-slate-200 bg-white px-2.5 font-mono text-[11px] text-slate-700 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-200"
                  />
                  <button
                    type="button"
                    class="shrink-0 cursor-pointer rounded-md border border-slate-200 bg-white px-2.5 py-1.5 text-[11px] text-slate-700 transition-colors hover:bg-slate-100 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-300"
                    @click="copyToken(local.browserTakeoverToken)"
                  >
                    复制
                  </button>
                </div>
              </div>
              <p class="text-[10px] leading-relaxed text-slate-400">
                启用后，配套浏览器扩展（仓库 browser-extension/ 目录，开发者模式加载）会把浏览器新下载自动转交 VortexDL。
              </p>
            </div>

            <p class="rounded-md border border-amber-200 bg-amber-50 p-2.5 text-[11px] text-amber-700 dark:border-amber-500/30 dark:bg-amber-500/10 dark:text-amber-300">
              默认策略：任务完成后立即停止做种。修改 BT 设置需重启应用生效。
            </p>
          </div>

          <!-- 外观 -->
          <div v-else class="space-y-4">
            <div class="space-y-1.5">
              <label class="font-semibold text-slate-700 dark:text-slate-300">界面主题</label>
              <div class="flex gap-2">
                <button
                  class="flex-1 cursor-pointer rounded-lg border p-3 text-left transition-colors"
                  :class="local.theme === 'dark' ? 'border-blue-500 bg-blue-500/10' : 'border-slate-200 hover:border-slate-400 dark:border-slate-700'"
                  @click="local.theme = 'dark'"
                >
                  <div class="mb-1 h-10 rounded-md bg-slate-950" />
                  <div class="text-[11px] font-medium">深色（推荐）</div>
                </button>
                <button
                  class="flex-1 cursor-pointer rounded-lg border p-3 text-left transition-colors"
                  :class="local.theme === 'light' ? 'border-blue-500 bg-blue-500/10' : 'border-slate-200 hover:border-slate-400 dark:border-slate-700'"
                  @click="local.theme = 'light'"
                >
                  <div class="mb-1 h-10 rounded-md bg-slate-100" />
                  <div class="text-[11px] font-medium">浅色</div>
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>

      <div class="flex items-center justify-between border-t border-slate-200 px-5 py-3 dark:border-slate-800">
        <span v-if="savedSuccess" class="text-xs font-medium text-emerald-600 dark:text-emerald-400">✓ 首选项已成功更新</span>
        <span v-else />
        <div class="flex gap-2.5">
          <button
            class="cursor-pointer rounded-md px-4 py-2 text-xs font-medium text-slate-600 transition-colors hover:bg-slate-100 dark:text-slate-400 dark:hover:bg-slate-800"
            @click="emit('close')"
          >
            取消
          </button>
          <button
            class="cursor-pointer rounded-md bg-blue-600 px-5 py-2 text-xs font-semibold text-white shadow-xs transition-colors hover:bg-blue-700"
            @click="save"
          >
            保存设置
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
