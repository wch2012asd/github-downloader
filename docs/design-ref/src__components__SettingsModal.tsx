import React from 'react';
import { 
  X, 
  Settings2, 
  Cpu, 
  Volume2, 
  Clock, 
  Check, 
  ShieldCheck,
  Zap
} from 'lucide-react';

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
  threads: number;
  onChangeThreads: (t: number) => void;
  autoClipboard: boolean;
  onToggleAutoClipboard: (val: boolean) => void;
  soundEnabled: boolean;
  onToggleSound: (val: boolean) => void;
  timeoutSeconds: number;
  onChangeTimeout: (sec: number) => void;
}

export const SettingsModal: React.FC<SettingsModalProps> = ({
  isOpen,
  onClose,
  threads,
  onChangeThreads,
  autoClipboard,
  onToggleAutoClipboard,
  soundEnabled,
  onToggleSound,
  timeoutSeconds,
  onChangeTimeout,
}) => {
  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-xs animate-in fade-in duration-200">
      <div className="bg-white dark:bg-neutral-900 border border-neutral-200 dark:border-neutral-800 rounded-xl shadow-xl w-full max-w-md overflow-hidden flex flex-col">
        {/* Header */}
        <div className="px-4 py-3 border-b border-neutral-200 dark:border-neutral-800 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Settings2 className="w-4 h-4 text-blue-500" />
            <h3 className="text-sm font-semibold text-neutral-900 dark:text-neutral-100">
              系统首选项与加速配置
            </h3>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="text-neutral-400 hover:text-neutral-600 dark:hover:text-neutral-200 p-1 rounded-md transition-colors cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Form Body */}
        <div className="p-4 space-y-4 text-xs">
          {/* Threads */}
          <div>
            <label className="block font-medium text-neutral-800 dark:text-neutral-200 mb-1.5">
              多线程并发分块数 (分段下载):
            </label>
            <div className="grid grid-cols-4 gap-2">
              {[4, 8, 16, 32].map((t) => (
                <button
                  key={t}
                  type="button"
                  onClick={() => onChangeThreads(t)}
                  className={`py-1.5 rounded-lg border font-mono font-medium transition-all cursor-pointer ${
                    threads === t
                      ? 'bg-blue-600 text-white border-blue-600 shadow-xs'
                      : 'bg-neutral-50 dark:bg-neutral-800 border-neutral-200 dark:border-neutral-700 text-neutral-700 dark:text-neutral-300 hover:bg-neutral-100 dark:hover:bg-neutral-700'
                  }`}
                >
                  {t} 线程
                </button>
              ))}
            </div>
            <p className="text-[11px] text-neutral-400 mt-1">
              针对大文件可开启 16 或 32 线程，充分压榨带宽吞吐率。
            </p>
          </div>

          {/* Connection Timeout */}
          <div>
            <label className="block font-medium text-neutral-800 dark:text-neutral-200 mb-1.5">
              网络连接请求超时 (秒):
            </label>
            <div className="flex items-center gap-2">
              <input
                type="range"
                min="5"
                max="60"
                step="5"
                value={timeoutSeconds}
                onChange={(e) => onChangeTimeout(Number(e.target.value))}
                className="flex-1 accent-blue-600 cursor-pointer"
              />
              <span className="font-mono font-medium text-neutral-700 dark:text-neutral-300 w-10 text-right">
                {timeoutSeconds}s
              </span>
            </div>
          </div>

          {/* Toggles */}
          <div className="space-y-2.5 pt-2 border-t border-neutral-100 dark:border-neutral-800">
            <label className="flex items-center justify-between cursor-pointer py-1">
              <div>
                <div className="font-medium text-neutral-800 dark:text-neutral-200">
                  剪贴板自动监测
                </div>
                <div className="text-[11px] text-neutral-400">
                  复制 GitHub 链接时自动填入并提示解析
                </div>
              </div>
              <input
                type="checkbox"
                checked={autoClipboard}
                onChange={(e) => onToggleAutoClipboard(e.target.checked)}
                className="rounded border-neutral-300 text-blue-600 focus:ring-blue-500 dark:border-neutral-700 dark:bg-neutral-800 cursor-pointer h-4 w-4"
              />
            </label>

            <label className="flex items-center justify-between cursor-pointer py-1">
              <div>
                <div className="font-medium text-neutral-800 dark:text-neutral-200">
                  下载完成提示音
                </div>
                <div className="text-[11px] text-neutral-400">
                  任务完成后播放柔和提示音响铃
                </div>
              </div>
              <input
                type="checkbox"
                checked={soundEnabled}
                onChange={(e) => onToggleSound(e.target.checked)}
                className="rounded border-neutral-300 text-blue-600 focus:ring-blue-500 dark:border-neutral-700 dark:bg-neutral-800 cursor-pointer h-4 w-4"
              />
            </label>
          </div>
        </div>

        {/* Footer */}
        <div className="px-4 py-2.5 border-t border-neutral-200 dark:border-neutral-800 bg-neutral-50 dark:bg-neutral-900/50 flex justify-end">
          <button
            type="button"
            onClick={onClose}
            className="px-4 py-1.5 rounded-lg bg-blue-600 hover:bg-blue-500 text-white text-xs font-medium transition-colors cursor-pointer"
          >
            保存并关闭
          </button>
        </div>
      </div>
    </div>
  );
};
