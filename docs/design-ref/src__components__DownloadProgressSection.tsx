import React from 'react';
import { 
  Play, 
  Pause, 
  Square, 
  FolderCheck, 
  CheckCircle2, 
  AlertCircle, 
  Gauge, 
  Clock, 
  Cpu,
  Layers,
  Sparkles,
  RotateCcw
} from 'lucide-react';
import { DownloadStatus } from '../types';
import { formatBytes, formatEta, formatSpeed } from '../utils/github';

interface DownloadProgressSectionProps {
  status: DownloadStatus;
  progress: number;
  speedBps: number;
  bytesDownloaded: number;
  totalBytes: number;
  etaSeconds: number;
  threads: number;
  speedHistory: number[];
  onStart: () => void;
  onPause: () => void;
  onResume: () => void;
  onCancel: () => void;
  onOpenFolder: () => void;
  onRedownload: () => void;
  onDownloadDirectly?: () => void;
}

export const DownloadProgressSection: React.FC<DownloadProgressSectionProps> = ({
  status,
  progress,
  speedBps,
  bytesDownloaded,
  totalBytes,
  etaSeconds,
  threads,
  speedHistory,
  onStart,
  onPause,
  onResume,
  onCancel,
  onOpenFolder,
  onRedownload,
  onDownloadDirectly,
}) => {
  const getStatusBadge = () => {
    switch (status) {
      case 'downloading':
        return (
          <span className="flex items-center gap-1.5 text-blue-600 dark:text-blue-400 font-medium">
            <span className="w-2 h-2 rounded-full bg-blue-500 animate-pulse" />
            <span>下载中...</span>
          </span>
        );
      case 'paused':
        return (
          <span className="flex items-center gap-1.5 text-amber-600 dark:text-amber-400 font-medium">
            <span className="w-2 h-2 rounded-full bg-amber-500" />
            <span>已暂停</span>
          </span>
        );
      case 'completed':
        return (
          <span className="flex items-center gap-1.5 text-emerald-600 dark:text-emerald-400 font-medium">
            <CheckCircle2 className="w-3.5 h-3.5" />
            <span>下载完成</span>
          </span>
        );
      case 'error':
        return (
          <span className="flex items-center gap-1.5 text-red-500 font-medium">
            <AlertCircle className="w-3.5 h-3.5" />
            <span>下载失败</span>
          </span>
        );
      case 'parsing':
        return (
          <span className="flex items-center gap-1.5 text-indigo-500 font-medium">
            <span>正在建立连接...</span>
          </span>
        );
      default:
        return (
          <span className="text-neutral-500 dark:text-neutral-400 font-medium">
            就绪
          </span>
        );
    }
  };

  // Sparkline maximum calculation
  const maxSpeed = Math.max(...speedHistory, 1024 * 1024 * 5); // at least 5 MB scale

  return (
    <section className="bg-white dark:bg-neutral-900 rounded-xl border border-neutral-200/80 dark:border-neutral-800 p-3 sm:p-3.5 shadow-xs transition-colors shrink-0">
      {/* Section Header */}
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-1.5">
          <Gauge className="w-3.5 h-3.5 text-emerald-500" />
          <h2 className="text-xs font-semibold text-neutral-900 dark:text-neutral-100 tracking-tight">
            下载进度与传输状态
          </h2>
        </div>

        {/* Status text */}
        <div className="text-xs font-medium">
          {getStatusBadge()}
        </div>
      </div>

      {/* Progress Bar Container */}
      <div className="space-y-1.5">
        <div className="relative w-full h-2.5 bg-neutral-100 dark:bg-neutral-800 rounded-full overflow-hidden border border-neutral-200/70 dark:border-neutral-700/60 p-[1px]">
          <div
            className={`h-full rounded-full transition-all duration-300 ease-out relative ${
              status === 'completed'
                ? 'bg-emerald-500'
                : status === 'paused'
                ? 'bg-amber-500'
                : status === 'error'
                ? 'bg-red-500'
                : 'bg-gradient-to-r from-blue-600 to-indigo-500'
            }`}
            style={{ width: `${Math.min(100, Math.max(0, progress))}%` }}
          >
            {/* Animated shimmer when actively downloading */}
            {status === 'downloading' && (
              <div className="absolute inset-0 bg-white/20 animate-[pulse_1.5s_infinite]" />
            )}
          </div>
        </div>

        {/* Telemetry Metrics Row */}
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 pt-0.5 text-xs">
          {/* Progress Percent */}
          <div className="flex flex-col">
            <span className="text-[10px] text-neutral-400 dark:text-neutral-500">完成度</span>
            <span className="font-mono font-bold text-sm text-neutral-900 dark:text-neutral-100 tabular-nums">
              {progress.toFixed(1)}%
            </span>
          </div>

          {/* Speed */}
          <div className="flex flex-col">
            <span className="text-[10px] text-neutral-400 dark:text-neutral-500">当前速度</span>
            <div className="flex items-center gap-1 font-mono font-semibold text-sm text-blue-600 dark:text-blue-400 tabular-nums">
              <span>{formatSpeed(speedBps)}</span>
            </div>
          </div>

          {/* Transferred / Total */}
          <div className="flex flex-col">
            <span className="text-[10px] text-neutral-400 dark:text-neutral-500">已传输 / 总大小</span>
            <span className="font-mono text-[11px] font-medium text-neutral-700 dark:text-neutral-300 tabular-nums mt-0.5">
              {totalBytes > 0 ? `${formatBytes(bytesDownloaded)} / ${formatBytes(totalBytes)}` : '-- / --'}
            </span>
          </div>

          {/* ETA & Threads */}
          <div className="flex flex-col">
            <span className="text-[10px] text-neutral-400 dark:text-neutral-500">剩余时间 · 线程</span>
            <div className="flex items-center gap-1 font-mono text-[11px] font-medium text-neutral-700 dark:text-neutral-300 tabular-nums mt-0.5">
              <span>{status === 'downloading' ? formatEta(etaSeconds) : '--'}</span>
              <span className="text-neutral-300 dark:text-neutral-700">·</span>
              <span className="text-neutral-500">{threads}T</span>
            </div>
          </div>
        </div>
      </div>

      {/* Speed Sparkline curve if downloading or recently completed */}
      {speedHistory.length > 3 && (
        <div className="mt-2 pt-1.5 border-t border-neutral-100 dark:border-neutral-800/80">
          <div className="flex items-center justify-between text-[9px] text-neutral-400 mb-0.5">
            <span>速度波动曲线</span>
            <span className="font-mono">{formatSpeed(Math.max(...speedHistory))} 峰值</span>
          </div>
          <div className="h-6 flex items-end gap-0.5 bg-neutral-50 dark:bg-neutral-800/40 rounded px-1.5 py-0.5">
            {speedHistory.map((val, idx) => {
              const heightPct = Math.max(8, Math.min(100, (val / maxSpeed) * 100));
              return (
                <div
                  key={idx}
                  className="flex-1 rounded-t-xs transition-all duration-300 bg-blue-500/70 hover:bg-blue-600"
                  style={{ height: `${heightPct}%` }}
                  title={formatSpeed(val)}
                />
              );
            })}
          </div>
        </div>
      )}

      {/* Action Control Buttons */}
      <div className="mt-2.5 pt-2 border-t border-neutral-100 dark:border-neutral-800/80 flex flex-wrap items-center justify-between gap-1.5">
        <div className="flex items-center gap-1.5">
          {/* Start Download button */}
          {status !== 'downloading' && status !== 'paused' && (
            <button
              type="button"
              onClick={onStart}
              className="h-8 px-3 rounded-lg bg-blue-600 hover:bg-blue-500 active:bg-blue-700 text-white text-xs font-semibold flex items-center gap-1 transition-colors cursor-pointer shadow-xs"
            >
              <Play className="w-3 h-3 fill-current" />
              <span>开始下载</span>
            </button>
          )}

          {/* Pause / Resume */}
          {status === 'downloading' && (
            <button
              type="button"
              onClick={onPause}
              className="h-8 px-3 rounded-lg bg-amber-500 hover:bg-amber-600 text-white text-xs font-medium flex items-center gap-1 transition-colors cursor-pointer shadow-xs"
            >
              <Pause className="w-3 h-3 fill-current" />
              <span>暂停</span>
            </button>
          )}

          {status === 'paused' && (
            <button
              type="button"
              onClick={onResume}
              className="h-8 px-3 rounded-lg bg-blue-600 hover:bg-blue-500 text-white text-xs font-semibold flex items-center gap-1 transition-colors cursor-pointer shadow-xs"
            >
              <Play className="w-3 h-3 fill-current" />
              <span>继续</span>
            </button>
          )}

          {/* Cancel */}
          <button
            type="button"
            onClick={onCancel}
            disabled={status === 'idle'}
            className="h-8 px-2.5 rounded-lg bg-neutral-100 dark:bg-neutral-800 hover:bg-neutral-200 dark:hover:bg-neutral-700 text-neutral-700 dark:text-neutral-300 border border-neutral-300 dark:border-neutral-700 text-xs font-medium flex items-center gap-1 transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
          >
            <Square className="w-3 h-3" />
            <span>取消</span>
          </button>

          {/* Redownload when completed */}
          {status === 'completed' && (
            <button
              type="button"
              onClick={onRedownload}
              className="h-8 px-2.5 rounded-lg bg-neutral-100 dark:bg-neutral-800 hover:bg-neutral-200 dark:hover:bg-neutral-700 text-neutral-700 dark:text-neutral-300 border border-neutral-300 dark:border-neutral-700 text-xs font-medium flex items-center gap-1 transition-colors cursor-pointer"
            >
              <RotateCcw className="w-3 h-3" />
              <span>重下</span>
            </button>
          )}
        </div>

        {/* Action Right: Direct download & Open directory */}
        <div className="flex items-center gap-1.5">
          {onDownloadDirectly && (
            <button
              type="button"
              onClick={onDownloadDirectly}
              className="h-8 px-2.5 rounded-lg bg-emerald-50 dark:bg-emerald-950/40 hover:bg-emerald-100 dark:hover:bg-emerald-900/50 text-emerald-700 dark:text-emerald-300 border border-emerald-200 dark:border-emerald-800 text-xs font-medium flex items-center gap-1 transition-colors cursor-pointer"
              title="使用当前加速节点直接保存真实文件到本地电脑"
            >
              <Sparkles className="w-3 h-3 text-emerald-500" />
              <span>直链下载</span>
            </button>
          )}

          <button
            type="button"
            onClick={onOpenFolder}
            className="h-8 px-2.5 rounded-lg bg-neutral-100 dark:bg-neutral-800 hover:bg-neutral-200 dark:hover:bg-neutral-700 text-neutral-700 dark:text-neutral-300 border border-neutral-300 dark:border-neutral-700 text-xs font-medium flex items-center gap-1 transition-colors cursor-pointer"
          >
            <FolderCheck className="w-3 h-3 text-neutral-500" />
            <span>打开目录</span>
          </button>
        </div>
      </div>
    </section>
  );
};
