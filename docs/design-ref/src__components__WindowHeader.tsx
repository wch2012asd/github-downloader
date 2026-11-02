import React from 'react';
import { 
  Sun, 
  Moon, 
  Minus, 
  Square, 
  X, 
  Github, 
  History, 
  Settings2,
  Sparkles,
  Zap
} from 'lucide-react';

interface WindowHeaderProps {
  isDark: boolean;
  onToggleTheme: () => void;
  onOpenSettings: () => void;
  onOpenHistory: () => void;
  historyCount: number;
  engineName: string;
  proxyStatus: 'idle' | 'checking' | 'connected' | 'error';
  proxyPort: number;
}

export const WindowHeader: React.FC<WindowHeaderProps> = ({
  isDark,
  onToggleTheme,
  onOpenSettings,
  onOpenHistory,
  historyCount,
  engineName,
  proxyStatus,
  proxyPort,
}) => {
  return (
    <header className="h-11 bg-white/80 dark:bg-neutral-900/80 backdrop-blur-md border-b border-neutral-200 dark:border-neutral-800 px-3.5 flex items-center justify-between select-none shrink-0 transition-colors duration-200 z-30">
      {/* Left: App Identity */}
      <div className="flex items-center gap-2.5 min-w-0">
        <div className="w-6 h-6 rounded-md bg-neutral-900 dark:bg-white text-white dark:text-neutral-900 flex items-center justify-center shadow-xs">
          <Github className="w-3.5 h-3.5" />
        </div>
        <div className="flex items-center gap-2">
          <h1 className="text-xs font-semibold tracking-tight text-neutral-800 dark:text-neutral-200 whitespace-nowrap">
            GitHub 加速下载器
          </h1>
          <span className="text-[10px] font-medium font-mono px-1.5 py-0.5 rounded bg-neutral-100 dark:bg-neutral-800 text-neutral-500 dark:text-neutral-400 border border-neutral-200 dark:border-neutral-700/60">
            v2.4
          </span>
        </div>

        {/* Status Indicators */}
        <div className="hidden md:flex items-center gap-2 ml-3 text-[11px] text-neutral-500 dark:text-neutral-400">
          <span className="text-neutral-300 dark:text-neutral-700">|</span>
          <div className="flex items-center gap-1.5">
            <Zap className="w-3 h-3 text-blue-500" />
            <span className="truncate max-w-[130px]">{engineName}</span>
          </div>
          {proxyStatus === 'connected' && (
            <>
              <span className="text-neutral-300 dark:text-neutral-700">·</span>
              <span className="font-mono text-emerald-600 dark:text-emerald-400">
                :{proxyPort} 代理正常
              </span>
            </>
          )}
        </div>
      </div>

      {/* Right: Actions & Window Controls */}
      <div className="flex items-center gap-1">
        {/* History Button */}
        <button
          onClick={onOpenHistory}
          className="relative h-7 px-2.5 rounded-md text-xs font-medium text-neutral-600 dark:text-neutral-300 hover:bg-neutral-100 dark:hover:bg-neutral-800 transition-colors flex items-center gap-1.5 cursor-pointer"
          title="下载任务记录"
        >
          <History className="w-3.5 h-3.5" />
          <span className="hidden sm:inline">任务记录</span>
          {historyCount > 0 && (
            <span className="px-1.5 py-0.2 rounded-full text-[10px] font-mono bg-blue-100 text-blue-700 dark:bg-blue-900/60 dark:text-blue-300">
              {historyCount}
            </span>
          )}
        </button>

        {/* Settings Button */}
        <button
          onClick={onOpenSettings}
          className="h-7 w-7 rounded-md text-neutral-600 dark:text-neutral-300 hover:bg-neutral-100 dark:hover:bg-neutral-800 transition-colors flex items-center justify-center cursor-pointer"
          title="首选项与高级配置"
        >
          <Settings2 className="w-3.5 h-3.5" />
        </button>

        {/* Theme Toggle Button */}
        <button
          onClick={onToggleTheme}
          className="h-7 w-7 rounded-md text-neutral-600 dark:text-neutral-300 hover:bg-neutral-100 dark:hover:bg-neutral-800 transition-colors flex items-center justify-center cursor-pointer"
          title={isDark ? "切换为明亮模式" : "切换为深色模式"}
        >
          {isDark ? <Sun className="w-3.5 h-3.5 text-amber-400" /> : <Moon className="w-3.5 h-3.5 text-neutral-600" />}
        </button>

        {/* Divider */}
        <div className="h-4 w-px bg-neutral-200 dark:bg-neutral-800 mx-1" />

        {/* Window Chrome Controls */}
        <div className="flex items-center">
          <button
            className="w-8 h-7 flex items-center justify-center text-neutral-500 hover:text-neutral-800 hover:bg-neutral-200/60 dark:hover:text-neutral-200 dark:hover:bg-neutral-800 rounded-sm transition-colors cursor-pointer"
            title="最小化"
          >
            <Minus className="w-3 h-3" />
          </button>
          <button
            className="w-8 h-7 flex items-center justify-center text-neutral-500 hover:text-neutral-800 hover:bg-neutral-200/60 dark:hover:text-neutral-200 dark:hover:bg-neutral-800 rounded-sm transition-colors cursor-pointer"
            title="最大化"
          >
            <Square className="w-2.5 h-2.5" />
          </button>
          <button
            className="w-8 h-7 flex items-center justify-center text-neutral-500 hover:text-white hover:bg-red-500 dark:hover:bg-red-600 rounded-sm transition-colors cursor-pointer"
            title="关闭"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>
    </header>
  );
};
