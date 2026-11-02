import React from 'react';
import { 
  Link2, 
  Clipboard, 
  ArrowRight, 
  X, 
  FileCode, 
  Package, 
  FolderArchive, 
  FileText,
  ExternalLink,
  ChevronDown
} from 'lucide-react';
import { ParsedGitHubInfo } from '../types';
import { PRESET_DEMO_LINKS } from '../utils/github';

interface UrlInputSectionProps {
  url: string;
  onUrlChange: (newUrl: string) => void;
  onParse: () => void;
  onPaste: () => void;
  parsedInfo: ParsedGitHubInfo | null;
  onSelectPreset: (presetUrl: string) => void;
  onSelectAsset?: (assetName: string, downloadUrl: string, size: string) => void;
}

export const UrlInputSection: React.FC<UrlInputSectionProps> = ({
  url,
  onUrlChange,
  onParse,
  onPaste,
  parsedInfo,
  onSelectPreset,
  onSelectAsset,
}) => {
  const getResourceIcon = (type?: string) => {
    switch (type) {
      case 'release-asset':
        return <Package className="w-4 h-4 text-blue-500" />;
      case 'source-archive':
        return <FolderArchive className="w-4 h-4 text-amber-500" />;
      case 'raw-file':
        return <FileCode className="w-4 h-4 text-emerald-500" />;
      default:
        return <FileText className="w-4 h-4 text-neutral-500" />;
    }
  };

  return (
    <section className="bg-white dark:bg-neutral-900 rounded-xl border border-neutral-200/80 dark:border-neutral-800 p-3 sm:p-3.5 shadow-xs transition-colors shrink-0">
      {/* Header */}
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-1.5">
          <Link2 className="w-3.5 h-3.5 text-blue-500" />
          <h2 className="text-xs font-semibold text-neutral-900 dark:text-neutral-100 tracking-tight">
            GitHub 链接
          </h2>
          <span className="text-[11px] text-neutral-400 dark:text-neutral-500">
            · 自动识别 Release/源码/Raw
          </span>
        </div>

        {/* Demo Preset dropdown/buttons */}
        <div className="flex items-center gap-1">
          <span className="text-[10px] text-neutral-400 dark:text-neutral-500 hidden sm:inline">预设:</span>
          {PRESET_DEMO_LINKS.slice(0, 3).map((item, idx) => (
            <button
              key={idx}
              type="button"
              onClick={() => onSelectPreset(item.url)}
              className="text-[10px] px-1.5 py-0.5 rounded bg-neutral-100 dark:bg-neutral-800 text-neutral-600 dark:text-neutral-300 hover:bg-neutral-200 dark:hover:bg-neutral-700 transition-colors cursor-pointer truncate max-w-[90px]"
              title={item.name}
            >
              {item.name.split(' ')[0]}
            </button>
          ))}
        </div>
      </div>

      {/* URL Input Bar */}
      <div className="flex items-stretch gap-1.5">
        <div className="relative flex-1">
          <input
            type="text"
            value={url}
            onChange={(e) => onUrlChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                onParse();
              }
            }}
            placeholder="粘贴 GitHub 链接 (例如: releases/download/...)"
            className="w-full h-8.5 px-3 pr-7 rounded-lg bg-neutral-50 dark:bg-neutral-800/80 border border-neutral-300 dark:border-neutral-700/80 text-xs text-neutral-900 dark:text-neutral-100 placeholder-neutral-400 dark:placeholder-neutral-500 focus:outline-hidden focus:border-blue-500 font-mono"
          />
          {url && (
            <button
              type="button"
              onClick={() => onUrlChange('')}
              className="absolute right-2 top-1/2 -translate-y-1/2 text-neutral-400 hover:text-neutral-600 dark:hover:text-neutral-200 cursor-pointer p-0.5"
              title="清空链接"
            >
              <X className="w-3 h-3" />
            </button>
          )}
        </div>

        {/* Buttons: Paste & Parse */}
        <div className="flex items-center gap-1.5 shrink-0">
          <button
            type="button"
            onClick={onPaste}
            className="h-8.5 px-2.5 rounded-lg bg-neutral-100 dark:bg-neutral-800 hover:bg-neutral-200 dark:hover:bg-neutral-700 text-neutral-700 dark:text-neutral-200 text-xs font-medium flex items-center gap-1 transition-colors cursor-pointer border border-neutral-200 dark:border-neutral-700"
            title="从剪贴板粘贴"
          >
            <Clipboard className="w-3 h-3" />
            <span>粘贴</span>
          </button>

          <button
            type="button"
            onClick={onParse}
            className="h-8.5 px-3 rounded-lg bg-blue-600 hover:bg-blue-500 active:bg-blue-700 text-white text-xs font-semibold flex items-center gap-1 transition-colors cursor-pointer shadow-xs"
            title="解析下载链接信息"
          >
            <span>解析</span>
            <ArrowRight className="w-3 h-3" />
          </button>
        </div>
      </div>

      {/* Parse Result / Resource Details */}
      <div className="mt-2 pt-2 border-t border-neutral-100 dark:border-neutral-800/80 flex flex-wrap items-center justify-between gap-1 text-[11px]">
        <div className="flex items-center gap-1.5">
          <span className="text-neutral-500 dark:text-neutral-400 font-medium">资源类型:</span>
          {parsedInfo && parsedInfo.isValid ? (
            <div className="flex items-center gap-1 font-medium text-neutral-800 dark:text-neutral-200">
              {getResourceIcon(parsedInfo.type)}
              <span>{parsedInfo.typeLabel}</span>
              {parsedInfo.owner && (
                <>
                  <span className="text-neutral-300 dark:text-neutral-700">·</span>
                  <span className="font-mono text-neutral-600 dark:text-neutral-400">
                    {parsedInfo.owner}/{parsedInfo.repo}
                  </span>
                </>
              )}
              {parsedInfo.tagOrBranch && (
                <span className="text-[10px] font-mono text-blue-600 dark:text-blue-400">
                  @{parsedInfo.tagOrBranch}
                </span>
              )}
            </div>
          ) : (
            <span className="text-neutral-400 dark:text-neutral-500">
              {url ? '尚未解析（点击“解析”提取）' : '待输入'}
            </span>
          )}
        </div>

        {parsedInfo && parsedInfo.isValid && parsedInfo.estimatedSize && (
          <div className="text-neutral-500 dark:text-neutral-400 flex items-center gap-1">
            <span>预估大小:</span>
            <span className="font-mono font-medium text-neutral-700 dark:text-neutral-300">
              {parsedInfo.estimatedSize}
            </span>
          </div>
        )}
      </div>

      {/* If Release contains multiple selectable assets */}
      {parsedInfo?.assets && parsedInfo.assets.length > 0 && (
        <div className="mt-2.5 p-2.5 rounded-lg bg-neutral-50 dark:bg-neutral-800/50 border border-neutral-200/80 dark:border-neutral-700/60">
          <div className="text-[11px] font-medium text-neutral-500 dark:text-neutral-400 mb-1.5">
            检测到该 Release 包含以下可选发布资产 (点击快速切换):
          </div>
          <div className="flex flex-wrap gap-1.5">
            {parsedInfo.assets.map((asset, idx) => (
              <button
                key={idx}
                type="button"
                onClick={() => onSelectAsset?.(asset.name, asset.downloadUrl, asset.size)}
                className="text-xs px-2.5 py-1 rounded bg-white dark:bg-neutral-800 border border-neutral-200 dark:border-neutral-700 hover:border-blue-500 dark:hover:border-blue-500 text-neutral-700 dark:text-neutral-300 flex items-center gap-1.5 transition-all cursor-pointer"
              >
                <Package className="w-3 h-3 text-neutral-400" />
                <span className="font-mono">{asset.name}</span>
                <span className="text-[10px] text-neutral-400 font-mono">({asset.size})</span>
              </button>
            ))}
          </div>
        </div>
      )}
    </section>
  );
};
