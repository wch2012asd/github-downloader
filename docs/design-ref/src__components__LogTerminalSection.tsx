import React, { useState, useRef, useEffect } from 'react';
import { 
  Terminal, 
  Trash2, 
  Copy, 
  Check, 
  Download, 
  ArrowDownToLine, 
  Search,
  Filter
} from 'lucide-react';
import { LogEntry, LogLevel } from '../types';

interface LogTerminalSectionProps {
  logs: LogEntry[];
  onClearLogs: () => void;
  onAddTestLog?: () => void;
}

export const LogTerminalSection: React.FC<LogTerminalSectionProps> = ({
  logs,
  onClearLogs,
}) => {
  const [filterLevel, setFilterLevel] = useState<string>('ALL');
  const [searchQuery, setSearchQuery] = useState<string>('');
  const [autoScroll, setAutoScroll] = useState<boolean>(true);
  const [copied, setCopied] = useState<boolean>(false);
  const terminalEndRef = useRef<HTMLDivElement>(null);

  // Auto-scroll logic
  useEffect(() => {
    if (autoScroll && terminalEndRef.current) {
      terminalEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [logs, autoScroll]);

  // Copy logs
  const handleCopyLogs = () => {
    const text = logs
      .map((l) => `${l.timestamp} [${l.level}] ${l.message}`)
      .join('\n');
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  // Export logs to txt
  const handleExportLogs = () => {
    const text = logs
      .map((l) => `${l.timestamp} [${l.level}] ${l.message}`)
      .join('\n');
    const blob = new Blob([text], { type: 'text/plain;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `github-downloader-log-${Date.now()}.txt`;
    a.click();
    URL.revokeObjectURL(url);
  };

  // Filter logs
  const filteredLogs = logs.filter((log) => {
    if (filterLevel !== 'ALL' && log.level !== filterLevel) return false;
    if (searchQuery && !log.message.toLowerCase().includes(searchQuery.toLowerCase())) return false;
    return true;
  });

  const getLevelStyle = (level: LogLevel) => {
    switch (level) {
      case 'SUCCESS':
        return 'text-emerald-400 font-semibold';
      case 'WARN':
        return 'text-amber-400 font-semibold';
      case 'ERROR':
        return 'text-rose-400 font-semibold';
      case 'PROXY':
        return 'text-indigo-400 font-semibold';
      case 'INFO':
      default:
        return 'text-blue-400 font-semibold';
    }
  };

  return (
    <section className="bg-white dark:bg-neutral-900 rounded-xl border border-neutral-200/80 dark:border-neutral-800 p-3 sm:p-3.5 shadow-xs transition-colors flex-1 flex flex-col min-h-0">
      {/* Title & Toolbar */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-1.5 mb-2">
        <div className="flex items-center gap-1.5">
          <Terminal className="w-3.5 h-3.5 text-neutral-500" />
          <h2 className="text-xs font-semibold text-neutral-900 dark:text-neutral-100 tracking-tight">
            运行日志
          </h2>
          <span className="text-[10px] font-mono text-neutral-400 dark:text-neutral-500">
            ({logs.length})
          </span>
        </div>

        {/* Action Buttons */}
        <div className="flex items-center flex-wrap gap-1">
          {/* Search */}
          <div className="relative">
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="搜索..."
              className="h-6.5 w-24 px-1.5 pl-5 rounded bg-neutral-100 dark:bg-neutral-800 text-[10px] text-neutral-800 dark:text-neutral-200 placeholder-neutral-400 border border-neutral-200 dark:border-neutral-700/60 focus:outline-hidden focus:border-blue-500 font-mono"
            />
            <Search className="w-2.5 h-2.5 text-neutral-400 absolute left-1.5 top-1/2 -translate-y-1/2" />
          </div>

          {/* Level Filter */}
          <select
            value={filterLevel}
            onChange={(e) => setFilterLevel(e.target.value)}
            className="h-6.5 px-1.5 rounded bg-neutral-100 dark:bg-neutral-800 text-[10px] text-neutral-700 dark:text-neutral-300 border border-neutral-200 dark:border-neutral-700/60 focus:outline-hidden cursor-pointer"
          >
            <option value="ALL">全部</option>
            <option value="INFO">INFO</option>
            <option value="SUCCESS">SUCCESS</option>
            <option value="WARN">WARN</option>
            <option value="ERROR">ERROR</option>
            <option value="PROXY">PROXY</option>
          </select>

          {/* Auto Scroll Toggle */}
          <button
            type="button"
            onClick={() => setAutoScroll(!autoScroll)}
            className={`h-6.5 px-1.5 rounded text-[10px] font-medium flex items-center gap-1 transition-colors cursor-pointer border ${
              autoScroll
                ? 'bg-blue-50 text-blue-600 dark:bg-blue-950/60 dark:text-blue-400 border-blue-200 dark:border-blue-800'
                : 'bg-neutral-100 dark:bg-neutral-800 text-neutral-600 dark:text-neutral-400 border-neutral-200 dark:border-neutral-700'
            }`}
            title="新日志生成时自动滚动至底部"
          >
            <ArrowDownToLine className="w-2.5 h-2.5" />
            <span className="hidden sm:inline">滚屏</span>
          </button>

          {/* Copy Button */}
          <button
            type="button"
            onClick={handleCopyLogs}
            className="h-6.5 px-1.5 rounded bg-neutral-100 dark:bg-neutral-800 hover:bg-neutral-200 dark:hover:bg-neutral-700 text-neutral-600 dark:text-neutral-300 border border-neutral-200 dark:border-neutral-700 text-[10px] font-medium flex items-center gap-1 transition-colors cursor-pointer"
            title="复制日志"
          >
            {copied ? <Check className="w-2.5 h-2.5 text-emerald-500" /> : <Copy className="w-2.5 h-2.5" />}
            <span>{copied ? '已复制' : '复制'}</span>
          </button>

          {/* Export Button */}
          <button
            type="button"
            onClick={handleExportLogs}
            className="h-6.5 w-6.5 rounded bg-neutral-100 dark:bg-neutral-800 hover:bg-neutral-200 dark:hover:bg-neutral-700 text-neutral-600 dark:text-neutral-300 border border-neutral-200 dark:border-neutral-700 flex items-center justify-center transition-colors cursor-pointer"
            title="导出 .txt"
          >
            <Download className="w-2.5 h-2.5" />
          </button>

          {/* Clear Button */}
          <button
            type="button"
            onClick={onClearLogs}
            className="h-6.5 w-6.5 rounded bg-neutral-100 dark:bg-neutral-800 hover:bg-rose-100 dark:hover:bg-rose-950/40 text-neutral-600 dark:text-neutral-300 hover:text-rose-600 dark:hover:text-rose-400 border border-neutral-200 dark:border-neutral-700 flex items-center justify-center transition-colors cursor-pointer"
            title="清空日志"
          >
            <Trash2 className="w-2.5 h-2.5" />
          </button>
        </div>
      </div>

      {/* Terminal Display Box */}
      <div className="w-full flex-1 min-h-[140px] rounded-lg bg-neutral-950 text-neutral-100 p-2.5 font-mono text-[11px] leading-relaxed overflow-y-auto border border-neutral-800/80 shadow-inner select-text">
        {filteredLogs.length === 0 ? (
          <div className="h-full flex items-center justify-center text-neutral-500 italic">
            暂无匹配日志记录
          </div>
        ) : (
          <div className="space-y-1">
            {filteredLogs.map((log) => (
              <div key={log.id} className="flex items-start gap-2 hover:bg-white/5 px-1 py-0.5 rounded transition-colors">
                <span className="text-neutral-500 shrink-0 select-none">
                  {log.timestamp}
                </span>
                <span className={`shrink-0 ${getLevelStyle(log.level)}`}>
                  [{log.level}]
                </span>
                <span className="text-neutral-300 break-all">
                  {log.message}
                </span>
              </div>
            ))}
            <div ref={terminalEndRef} />
          </div>
        )}
      </div>
    </section>
  );
};
