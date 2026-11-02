import React from 'react';
import { 
  X, 
  Trash2, 
  RotateCcw, 
  ExternalLink, 
  FolderCheck, 
  CheckCircle2, 
  FileCode,
  Package
} from 'lucide-react';
import { DownloadTask } from '../types';
import { formatBytes } from '../utils/github';

interface HistoryModalProps {
  isOpen: boolean;
  onClose: () => void;
  history: DownloadTask[];
  onClearHistory: () => void;
  onRedownload: (task: DownloadTask) => void;
}

export const HistoryModal: React.FC<HistoryModalProps> = ({
  isOpen,
  onClose,
  history,
  onClearHistory,
  onRedownload,
}) => {
  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-xs animate-in fade-in duration-200">
      <div className="bg-white dark:bg-neutral-900 border border-neutral-200 dark:border-neutral-800 rounded-xl shadow-xl w-full max-w-lg overflow-hidden flex flex-col max-h-[80vh]">
        {/* Header */}
        <div className="px-4 py-3 border-b border-neutral-200 dark:border-neutral-800 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <h3 className="text-sm font-semibold text-neutral-900 dark:text-neutral-100">
              下载历史记录
            </h3>
            <span className="text-xs font-mono text-neutral-400">
              ({history.length} 个任务)
            </span>
          </div>

          <div className="flex items-center gap-1.5">
            {history.length > 0 && (
              <button
                type="button"
                onClick={onClearHistory}
                className="text-xs text-rose-500 hover:text-rose-600 px-2 py-1 rounded hover:bg-rose-50 dark:hover:bg-rose-950/30 transition-colors cursor-pointer flex items-center gap-1"
              >
                <Trash2 className="w-3 h-3" />
                <span>清空记录</span>
              </button>
            )}
            <button
              type="button"
              onClick={onClose}
              className="text-neutral-400 hover:text-neutral-600 dark:hover:text-neutral-200 p-1 rounded-md transition-colors cursor-pointer"
            >
              <X className="w-4 h-4" />
            </button>
          </div>
        </div>

        {/* List Content */}
        <div className="p-4 overflow-y-auto flex-1 space-y-2.5">
          {history.length === 0 ? (
            <div className="py-12 text-center text-xs text-neutral-400 dark:text-neutral-500">
              暂无已完成的下载任务
            </div>
          ) : (
            history.map((task) => (
              <div
                key={task.id}
                className="p-3 rounded-lg border border-neutral-200/80 dark:border-neutral-800 bg-neutral-50/70 dark:bg-neutral-800/50 flex items-center justify-between gap-3 text-xs"
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2 font-medium text-neutral-800 dark:text-neutral-200 truncate mb-1">
                    <Package className="w-3.5 h-3.5 text-blue-500 shrink-0" />
                    <span className="truncate font-mono">{task.filename}</span>
                  </div>
                  <div className="flex items-center gap-2 text-[11px] text-neutral-400 dark:text-neutral-500">
                    <span className="font-mono">{formatBytes(task.totalBytes)}</span>
                    <span>·</span>
                    <span>{new Date(task.completedAt || task.createdAt).toLocaleTimeString()}</span>
                    <span>·</span>
                    <span className="truncate max-w-[140px]">{task.savePath}</span>
                  </div>
                </div>

                <div className="flex items-center gap-1.5 shrink-0">
                  <button
                    type="button"
                    onClick={() => onRedownload(task)}
                    className="p-1.5 rounded hover:bg-neutral-200 dark:hover:bg-neutral-700 text-neutral-600 dark:text-neutral-300 transition-colors cursor-pointer"
                    title="重新下载"
                  >
                    <RotateCcw className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>
            ))
          )}
        </div>

        {/* Footer */}
        <div className="px-4 py-2.5 border-t border-neutral-200 dark:border-neutral-800 bg-neutral-50 dark:bg-neutral-900/50 flex justify-end">
          <button
            type="button"
            onClick={onClose}
            className="px-3.5 py-1.5 rounded-lg bg-neutral-200 dark:bg-neutral-800 hover:bg-neutral-300 dark:hover:bg-neutral-700 text-xs font-medium text-neutral-800 dark:text-neutral-200 transition-colors cursor-pointer"
          >
            关闭
          </button>
        </div>
      </div>
    </div>
  );
};
