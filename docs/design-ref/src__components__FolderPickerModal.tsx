import React, { useState } from 'react';
import { 
  X, 
  Folder, 
  FolderPlus, 
  Check, 
  HardDrive, 
  ChevronRight,
  Laptop
} from 'lucide-react';

interface FolderPickerModalProps {
  isOpen: boolean;
  onClose: () => void;
  currentPath: string;
  onSelectPath: (path: string) => void;
}

const DIRECTORY_TREE = [
  {
    drive: 'D: 本地磁盘',
    folders: [
      'D:\\MP\\github-downloader\\dist',
      'D:\\Downloads\\GitHub',
      'D:\\Develop\\Tools',
      'D:\\Software\\Installers',
    ],
  },
  {
    drive: 'C: 系统盘',
    folders: [
      'C:\\Users\\User\\Downloads',
      'C:\\Users\\User\\Desktop',
      'C:\\Users\\User\\Documents',
    ],
  },
];

export const FolderPickerModal: React.FC<FolderPickerModalProps> = ({
  isOpen,
  onClose,
  currentPath,
  onSelectPath,
}) => {
  const [selected, setSelected] = useState(currentPath);
  const [customInput, setCustomInput] = useState(currentPath);

  if (!isOpen) return null;

  const handleConfirm = () => {
    onSelectPath(customInput || selected);
    onClose();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-xs animate-in fade-in duration-200">
      <div className="bg-white dark:bg-neutral-900 border border-neutral-200 dark:border-neutral-800 rounded-xl shadow-xl w-full max-w-md overflow-hidden flex flex-col">
        {/* Header */}
        <div className="px-4 py-3 border-b border-neutral-200 dark:border-neutral-800 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Folder className="w-4 h-4 text-amber-500" />
            <h3 className="text-sm font-semibold text-neutral-900 dark:text-neutral-100">
              选择保存目标文件夹
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

        {/* Content */}
        <div className="p-4 space-y-3 text-xs">
          <div>
            <label className="block text-[11px] font-medium text-neutral-500 dark:text-neutral-400 mb-1">
              当前选定目录:
            </label>
            <input
              type="text"
              value={customInput}
              onChange={(e) => {
                setCustomInput(e.target.value);
                setSelected(e.target.value);
              }}
              className="w-full h-8 px-2.5 rounded-lg bg-neutral-50 dark:bg-neutral-800 border border-neutral-300 dark:border-neutral-700 text-xs font-mono text-neutral-800 dark:text-neutral-200 focus:outline-hidden focus:border-blue-500"
            />
          </div>

          <div className="border border-neutral-200 dark:border-neutral-800 rounded-lg p-2 max-h-52 overflow-y-auto space-y-2 bg-neutral-50/50 dark:bg-neutral-950/40 font-mono text-[11px]">
            {DIRECTORY_TREE.map((group) => (
              <div key={group.drive}>
                <div className="flex items-center gap-1.5 font-semibold text-neutral-700 dark:text-neutral-300 py-1">
                  <HardDrive className="w-3.5 h-3.5 text-neutral-400" />
                  <span>{group.drive}</span>
                </div>
                <div className="pl-4 space-y-1">
                  {group.folders.map((folder) => (
                    <button
                      key={folder}
                      type="button"
                      onClick={() => {
                        setSelected(folder);
                        setCustomInput(folder);
                      }}
                      className={`w-full text-left px-2 py-1 rounded flex items-center justify-between transition-colors cursor-pointer ${
                        selected === folder
                          ? 'bg-blue-100 dark:bg-blue-900/50 text-blue-700 dark:text-blue-300 font-medium'
                          : 'hover:bg-neutral-100 dark:hover:bg-neutral-800 text-neutral-600 dark:text-neutral-400'
                      }`}
                    >
                      <div className="flex items-center gap-1.5 truncate">
                        <Folder className="w-3 h-3 text-amber-500 shrink-0" />
                        <span className="truncate">{folder}</span>
                      </div>
                      {selected === folder && (
                        <Check className="w-3.5 h-3.5 text-blue-600 dark:text-blue-400 shrink-0" />
                      )}
                    </button>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* Footer */}
        <div className="px-4 py-2.5 border-t border-neutral-200 dark:border-neutral-800 bg-neutral-50 dark:bg-neutral-900/50 flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="px-3 py-1.5 rounded-lg border border-neutral-300 dark:border-neutral-700 text-xs font-medium text-neutral-700 dark:text-neutral-300 hover:bg-neutral-100 dark:hover:bg-neutral-800 cursor-pointer"
          >
            取消
          </button>
          <button
            type="button"
            onClick={handleConfirm}
            className="px-4 py-1.5 rounded-lg bg-blue-600 hover:bg-blue-500 text-white text-xs font-semibold cursor-pointer"
          >
            确定选择
          </button>
        </div>
      </div>
    </div>
  );
};
