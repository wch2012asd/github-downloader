import React from 'react';
import { 
  FolderOpen, 
  FileText, 
  HardDrive, 
  Check, 
  FolderCheck,
  ChevronRight
} from 'lucide-react';

interface SaveLocationSectionProps {
  savePath: string;
  onSavePathChange: (path: string) => void;
  filename: string;
  onFilenameChange: (filename: string) => void;
  openFolderAfterDownload: boolean;
  onToggleOpenFolder: (val: boolean) => void;
  onBrowseFolder: () => void;
}

const COMMON_PATHS = [
  { label: '默认目录', path: 'D:\\MP\\github-downloader\\dist' },
  { label: '下载目录', path: 'C:\\Users\\User\\Downloads' },
  { label: '桌面', path: 'C:\\Users\\User\\Desktop' },
];

export const SaveLocationSection: React.FC<SaveLocationSectionProps> = ({
  savePath,
  onSavePathChange,
  filename,
  onFilenameChange,
  openFolderAfterDownload,
  onToggleOpenFolder,
  onBrowseFolder,
}) => {
  // Determine file extension
  const extension = filename.includes('.') ? filename.split('.').pop()?.toUpperCase() : '';

  return (
    <section className="bg-white dark:bg-neutral-900 rounded-xl border border-neutral-200/80 dark:border-neutral-800 p-3 sm:p-3.5 shadow-xs transition-colors shrink-0">
      {/* Header */}
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-1.5">
          <FolderOpen className="w-3.5 h-3.5 text-amber-500" />
          <h2 className="text-xs font-semibold text-neutral-900 dark:text-neutral-100 tracking-tight">
            保存位置与文件名
          </h2>
        </div>

        {/* Free space indicator */}
        <div className="flex items-center gap-1 text-[11px] text-neutral-500 dark:text-neutral-400">
          <HardDrive className="w-3 h-3 text-neutral-400" />
          <span>可用:</span>
          <span className="font-mono font-medium text-neutral-700 dark:text-neutral-300">
            218.4 GB
          </span>
        </div>
      </div>

      <div className="space-y-2">
        {/* Directory Row */}
        <div>
          <div className="flex items-center justify-between mb-1 text-[11px]">
            <label className="font-medium text-neutral-700 dark:text-neutral-300">
              存储目录:
            </label>
            {/* Quick path shortcuts */}
            <div className="flex items-center gap-1">
              <span className="text-[10px] text-neutral-400">快捷:</span>
              {COMMON_PATHS.map((item) => (
                <button
                  key={item.label}
                  type="button"
                  onClick={() => onSavePathChange(item.path)}
                  className={`text-[10px] px-1 py-0.2 rounded transition-colors cursor-pointer ${
                    savePath === item.path
                      ? 'bg-neutral-200 dark:bg-neutral-700 text-neutral-900 dark:text-neutral-100 font-medium'
                      : 'text-neutral-500 hover:text-neutral-900 dark:hover:text-neutral-200 hover:bg-neutral-100 dark:hover:bg-neutral-800'
                  }`}
                >
                  {item.label}
                </button>
              ))}
            </div>
          </div>

          <div className="flex items-center gap-1.5">
            <button
              type="button"
              onClick={onBrowseFolder}
              className="h-8 px-2.5 rounded-lg bg-neutral-100 dark:bg-neutral-800 hover:bg-neutral-200 dark:hover:bg-neutral-700 text-neutral-700 dark:text-neutral-200 border border-neutral-300 dark:border-neutral-700 text-xs font-medium flex items-center gap-1 transition-colors cursor-pointer shrink-0"
              title="选择保存目录"
            >
              <FolderOpen className="w-3 h-3" />
              <span>选择</span>
            </button>

            <input
              type="text"
              value={savePath}
              onChange={(e) => onSavePathChange(e.target.value)}
              placeholder="请输入或选择下载存储目录路径"
              className="flex-1 h-8 px-2.5 rounded-lg bg-neutral-50 dark:bg-neutral-800/80 border border-neutral-300 dark:border-neutral-700/80 text-xs font-mono text-neutral-800 dark:text-neutral-200 focus:outline-hidden focus:border-blue-500"
            />
          </div>
        </div>

        {/* Filename Row */}
        <div>
          <div className="relative flex items-center">
            <input
              type="text"
              value={filename}
              onChange={(e) => onFilenameChange(e.target.value)}
              placeholder="文件保存名称"
              className="w-full h-8 px-2.5 pr-14 rounded-lg bg-neutral-50 dark:bg-neutral-800/80 border border-neutral-300 dark:border-neutral-700/80 text-xs font-mono text-neutral-800 dark:text-neutral-200 focus:outline-hidden focus:border-blue-500"
            />
            {extension && (
              <span className="absolute right-1.5 text-[9px] font-mono px-1 py-0.2 rounded bg-neutral-200 dark:bg-neutral-700 text-neutral-700 dark:text-neutral-300 font-semibold uppercase">
                .{extension}
              </span>
            )}
          </div>
        </div>

        {/* Options Row */}
        <div className="pt-0.5 flex items-center justify-between text-[11px] text-neutral-600 dark:text-neutral-400">
          <label className="flex items-center gap-1.5 cursor-pointer select-none">
            <input
              type="checkbox"
              checked={openFolderAfterDownload}
              onChange={(e) => onToggleOpenFolder(e.target.checked)}
              className="rounded border-neutral-300 text-blue-600 focus:ring-blue-500 dark:border-neutral-700 dark:bg-neutral-800 cursor-pointer h-3.5 w-3.5"
            />
            <span>下载完成后自动打开文件目录</span>
          </label>
          <span className="text-[10px] text-neutral-400 dark:text-neutral-500 font-mono truncate max-w-[200px]">
            {savePath}\{filename || '...'}
          </span>
        </div>
      </div>
    </section>
  );
};
