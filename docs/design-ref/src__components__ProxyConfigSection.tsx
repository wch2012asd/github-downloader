import React from 'react';
import { 
  Network, 
  RefreshCw, 
  Activity, 
  CheckCircle2, 
  XCircle, 
  Clock, 
  Zap,
  Globe,
  SlidersHorizontal
} from 'lucide-react';
import { AccelerationEngine, AccelerationOption, ProxySettings } from '../types';

interface ProxyConfigSectionProps {
  proxySettings: ProxySettings;
  onPortChange: (port: number) => void;
  onAutoDetect: () => void;
  onTestConnection: () => void;
  selectedEngine: AccelerationEngine;
  onSelectEngine: (engine: AccelerationEngine) => void;
}

const ACCELERATION_ENGINES: AccelerationOption[] = [
  {
    id: 'clash',
    name: 'Clash 本地代理',
    description: '通过本地 Clash / 客户端梯子转发流量',
    badge: '推荐',
    latencyHint: '约 45ms',
    pingState: 'fast',
  },
  {
    id: 'ghproxy',
    name: 'GHProxy 镜像',
    description: '国内高速分发 CDN 节点',
    badge: '免客户端',
    latencyHint: '约 68ms',
    pingState: 'fast',
  },
  {
    id: 'kkgithub',
    name: 'KKGitHub 节点',
    description: '全球负载均衡高速反代',
    badge: '极速',
    latencyHint: '约 72ms',
    pingState: 'medium',
  },
  {
    id: 'direct',
    name: '官方直连',
    description: '不经由加速节点，直接连接 GitHub 官方',
    badge: '原始',
    latencyHint: '可能受限',
    pingState: 'slow',
  },
];

export const ProxyConfigSection: React.FC<ProxyConfigSectionProps> = ({
  proxySettings,
  onPortChange,
  onAutoDetect,
  onTestConnection,
  selectedEngine,
  onSelectEngine,
}) => {
  return (
    <section className="bg-white dark:bg-neutral-900 rounded-xl border border-neutral-200/80 dark:border-neutral-800 p-3 sm:p-3.5 shadow-xs transition-colors shrink-0">
      {/* Title */}
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-1.5">
          <Network className="w-3.5 h-3.5 text-indigo-500" />
          <h2 className="text-xs font-semibold text-neutral-900 dark:text-neutral-100 tracking-tight">
            网络加速与 Clash 代理
          </h2>
        </div>

        {/* Status indicator */}
        <div className="flex items-center gap-1.5 text-[11px]">
          {proxySettings.status === 'checking' && (
            <span className="flex items-center gap-1 text-blue-600 dark:text-blue-400 font-medium">
              <RefreshCw className="w-3 h-3 animate-spin" />
              <span>检测中...</span>
            </span>
          )}
          {proxySettings.status === 'connected' && (
            <span className="flex items-center gap-1 text-emerald-600 dark:text-emerald-400 font-medium font-mono">
              <CheckCircle2 className="w-3 h-3 text-emerald-500" />
              <span>代理正常 ({proxySettings.latencyMs}ms)</span>
            </span>
          )}
          {proxySettings.status === 'error' && (
            <span className="flex items-center gap-1 text-red-500 dark:text-red-400 font-medium">
              <XCircle className="w-3 h-3" />
              <span>未检测到代理</span>
            </span>
          )}
          {proxySettings.status === 'idle' && (
            <span className="text-neutral-400 dark:text-neutral-500">
              待检测
            </span>
          )}
        </div>
      </div>

      <div className="space-y-2">
        {/* Clash Local Proxy Controls */}
        <div className="bg-neutral-50/70 dark:bg-neutral-800/40 rounded-lg p-2.5 border border-neutral-200/70 dark:border-neutral-800">
          <div className="flex flex-wrap items-center justify-between gap-1.5">
            <div className="flex items-center gap-1.5">
              <label htmlFor="port-input" className="text-xs text-neutral-600 dark:text-neutral-400 font-medium">
                端口:
              </label>
              <input
                id="port-input"
                type="number"
                min="1000"
                max="65535"
                value={proxySettings.port}
                onChange={(e) => onPortChange(Number(e.target.value) || 7890)}
                className="w-16 h-7.5 px-1.5 rounded-md bg-white dark:bg-neutral-800 border border-neutral-300 dark:border-neutral-700 text-xs font-mono font-medium text-neutral-800 dark:text-neutral-200 focus:outline-hidden focus:border-blue-500"
              />

              <button
                type="button"
                onClick={onAutoDetect}
                disabled={proxySettings.status === 'checking'}
                className="h-7.5 px-2 rounded-md bg-white dark:bg-neutral-800 hover:bg-neutral-100 dark:hover:bg-neutral-700 text-neutral-700 dark:text-neutral-200 border border-neutral-200 dark:border-neutral-700 text-[11px] font-medium flex items-center gap-1 transition-colors cursor-pointer disabled:opacity-50"
                title="自动扫描端口"
              >
                <RefreshCw className={`w-3 h-3 ${proxySettings.status === 'checking' ? 'animate-spin' : ''}`} />
                <span>自动检测</span>
              </button>

              <button
                type="button"
                onClick={onTestConnection}
                disabled={proxySettings.status === 'checking'}
                className="h-7.5 px-2 rounded-md bg-white dark:bg-neutral-800 hover:bg-neutral-100 dark:hover:bg-neutral-700 text-neutral-700 dark:text-neutral-200 border border-neutral-200 dark:border-neutral-700 text-[11px] font-medium flex items-center gap-1 transition-colors cursor-pointer disabled:opacity-50"
                title="测试延迟"
              >
                <Activity className="w-3 h-3 text-indigo-500" />
                <span>测试连接</span>
              </button>
            </div>

            {/* Quick port recommendations */}
            <div className="flex items-center gap-1 text-[10px] text-neutral-400 dark:text-neutral-500">
              <span>快捷:</span>
              {[7890, 7891, 10808].map((p) => (
                <button
                  key={p}
                  type="button"
                  onClick={() => onPortChange(p)}
                  className={`font-mono px-1 py-0.2 rounded transition-colors cursor-pointer ${
                    proxySettings.port === p
                      ? 'bg-blue-100 dark:bg-blue-900/40 text-blue-700 dark:text-blue-300 font-semibold'
                      : 'hover:text-neutral-800 dark:hover:text-neutral-200 hover:bg-neutral-200/50 dark:hover:bg-neutral-700'
                  }`}
                >
                  {p}
                </button>
              ))}
            </div>
          </div>
        </div>

        {/* Acceleration Mode / Mirror Selector */}
        <div>
          <div className="text-[11px] font-medium text-neutral-500 dark:text-neutral-400 mb-1 flex items-center justify-between">
            <span className="flex items-center gap-1">
              <Zap className="w-3 h-3 text-amber-500" />
              <span>加速引擎通道</span>
            </span>
            <span className="text-[10px] text-neutral-400">点击切换</span>
          </div>

          <div className="grid grid-cols-2 sm:grid-cols-4 gap-1.5">
            {ACCELERATION_ENGINES.map((engine) => {
              const isSelected = selectedEngine === engine.id;
              return (
                <button
                  key={engine.id}
                  type="button"
                  onClick={() => onSelectEngine(engine.id)}
                  className={`p-1.5 rounded-lg text-left transition-all cursor-pointer flex flex-col justify-between border ${
                    isSelected
                      ? 'bg-blue-50/60 dark:bg-blue-950/40 border-blue-500 dark:border-blue-500 shadow-xs ring-1 ring-blue-500/20'
                      : 'bg-white/60 dark:bg-neutral-800/60 border-neutral-200 dark:border-neutral-700/60 hover:border-neutral-300 dark:hover:border-neutral-600'
                  }`}
                >
                  <div className="flex items-center justify-between gap-1 mb-0.5">
                    <span className="text-[11px] font-semibold text-neutral-800 dark:text-neutral-100 truncate">
                      {engine.name}
                    </span>
                  </div>
                  <div className="flex items-center justify-between text-[10px] text-neutral-400 dark:text-neutral-500">
                    <span className="font-mono text-emerald-600 dark:text-emerald-400">
                      {engine.latencyHint}
                    </span>
                    <span className="text-[9px] px-1 rounded bg-neutral-100 dark:bg-neutral-700 font-mono">
                      {engine.badge}
                    </span>
                  </div>
                </button>
              );
            })}
          </div>
        </div>
      </div>
    </section>
  );
};
