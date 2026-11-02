import React, { useState, useEffect, useRef, useCallback } from 'react';
import { WindowHeader } from './components/WindowHeader';
import { UrlInputSection } from './components/UrlInputSection';
import { ProxyConfigSection } from './components/ProxyConfigSection';
import { SaveLocationSection } from './components/SaveLocationSection';
import { DownloadProgressSection } from './components/DownloadProgressSection';
import { LogTerminalSection } from './components/LogTerminalSection';
import { HistoryModal } from './components/HistoryModal';
import { SettingsModal } from './components/SettingsModal';
import { FolderPickerModal } from './components/FolderPickerModal';
import { 
  AccelerationEngine, 
  DownloadStatus, 
  DownloadTask, 
  LogEntry, 
  LogLevel, 
  ParsedGitHubInfo, 
  ProxySettings 
} from './types';
import { 
  buildAcceleratedUrl, 
  getNowTimestamp, 
  parseGitHubUrl, 
  PRESET_DEMO_LINKS 
} from './utils/github';
import { 
  Maximize2, 
  Minimize2, 
  CheckCircle, 
  AlertCircle, 
  Info,
  Layers,
  Sparkles
} from 'lucide-react';

export default function App() {
  // Theme state
  const [isDark, setIsDark] = useState<boolean>(() => {
    if (typeof window !== 'undefined') {
      const saved = localStorage.getItem('gh_theme');
      if (saved) return saved === 'dark';
      return window.matchMedia('(prefers-color-scheme: dark)').matches;
    }
    return true;
  });

  // Apply dark mode class to html element
  useEffect(() => {
    if (isDark) {
      document.documentElement.classList.add('dark');
      localStorage.setItem('gh_theme', 'dark');
    } else {
      document.documentElement.classList.remove('dark');
      localStorage.setItem('gh_theme', 'light');
    }
  }, [isDark]);

  // Window frame mode (Full screen or desktop window mode)
  const [isMaximized, setIsMaximized] = useState<boolean>(false);

  // GitHub URL and Parsed Information
  const initialDemo = PRESET_DEMO_LINKS[0];
  const [url, setUrl] = useState<string>(initialDemo.url);
  const [parsedInfo, setParsedInfo] = useState<ParsedGitHubInfo | null>(() => 
    parseGitHubUrl(initialDemo.url)
  );

  // Proxy Settings
  const [proxySettings, setProxySettings] = useState<ProxySettings>({
    port: 7890,
    protocol: 'HTTP',
    status: 'connected',
    latencyMs: 46,
    message: 'Clash 代理运行正常',
  });

  // Acceleration Engine
  const [selectedEngine, setSelectedEngine] = useState<AccelerationEngine>('clash');

  // Save Path & Filename
  const [savePath, setSavePath] = useState<string>('D:\\MP\\github-downloader\\dist');
  const [filename, setFilename] = useState<string>('LumaShot-Setup.exe');
  const [openFolderAfterDownload, setOpenFolderAfterDownload] = useState<boolean>(true);

  // Download Task State
  const [downloadStatus, setDownloadStatus] = useState<DownloadStatus>('idle');
  const [progress, setProgress] = useState<number>(0);
  const [speedBps, setSpeedBps] = useState<number>(0);
  const [bytesDownloaded, setBytesDownloaded] = useState<number>(0);
  const [totalBytes, setTotalBytes] = useState<number>(68984832); // 65.8 MB LumaShot
  const [etaSeconds, setEtaSeconds] = useState<number>(0);
  const [threads, setThreads] = useState<number>(8);
  const [speedHistory, setSpeedHistory] = useState<number[]>([]);

  // System Preferences
  const [autoClipboard, setAutoClipboard] = useState<boolean>(true);
  const [soundEnabled, setSoundEnabled] = useState<boolean>(true);
  const [timeoutSeconds, setTimeoutSeconds] = useState<number>(20);

  // Modals state
  const [isHistoryOpen, setIsHistoryOpen] = useState<boolean>(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState<boolean>(false);
  const [isFolderPickerOpen, setIsFolderPickerOpen] = useState<boolean>(false);
  const [toastMessage, setToastMessage] = useState<string | null>(null);

  // Task history list
  const [taskHistory, setTaskHistory] = useState<DownloadTask[]>([]);

  // Logs state initialized with real logs from original screenshot
  const [logs, setLogs] = useState<LogEntry[]>([
    {
      id: 'log-1',
      timestamp: '[10:00:10]',
      level: 'INFO',
      message: '测试端口 7890 的代理连接...',
    },
    {
      id: 'log-2',
      timestamp: '[10:00:12]',
      level: 'WARN',
      message: '代理连接失败 (端口 7890), 请检查 Clash 是否运行',
    },
    {
      id: 'log-3',
      timestamp: '[10:00:14]',
      level: 'INFO',
      message: '正在检测 Clash 代理端口...',
    },
    {
      id: 'log-4',
      timestamp: '[10:00:19]',
      level: 'WARN',
      message: '未检测到 Clash 代理，已自动切换备用 GHProxy 镜像通道',
    },
    {
      id: 'log-5',
      timestamp: '[10:00:22]',
      level: 'SUCCESS',
      message: '检测到 Clash 代理已启动于 127.0.0.1:7890 (延迟: 46ms)，网络就绪',
    },
    {
      id: 'log-6',
      timestamp: '[10:00:25]',
      level: 'INFO',
      message: '已载入目标: jimmgreen/LumaShot @v0.3.0 (LumaShot-Setup.exe, 约 65.8 MB)',
    },
  ]);

  const addLog = useCallback((level: LogLevel, message: string) => {
    setLogs((prev) => [
      ...prev,
      {
        id: `log-${Date.now()}-${Math.random().toString(36).substr(2, 4)}`,
        timestamp: getNowTimestamp(),
        level,
        message,
      },
    ]);
  }, []);

  const showToast = (msg: string) => {
    setToastMessage(msg);
    setTimeout(() => {
      setToastMessage(null);
    }, 3000);
  };

  // Play gentle completion sound
  const playFinishSound = () => {
    if (!soundEnabled || typeof window === 'undefined') return;
    try {
      const audioCtx = new (window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext)();
      const osc = audioCtx.createOscillator();
      const gain = audioCtx.createGain();
      osc.connect(gain);
      gain.connect(audioCtx.destination);
      osc.type = 'sine';
      osc.frequency.setValueAtTime(587.33, audioCtx.currentTime); // D5
      osc.frequency.setValueAtTime(880, audioCtx.currentTime + 0.1); // A5
      gain.gain.setValueAtTime(0.15, audioCtx.currentTime);
      gain.gain.exponentialRampToValueAtTime(0.001, audioCtx.currentTime + 0.5);
      osc.start();
      osc.stop(audioCtx.currentTime + 0.5);
    } catch {
      // AudioContext unavailable or blocked
    }
  };

  // Handle URL change & auto parse
  const handleUrlChange = (newUrl: string) => {
    setUrl(newUrl);
    if (!newUrl.trim()) {
      setParsedInfo(null);
      return;
    }
    const info = parseGitHubUrl(newUrl);
    setParsedInfo(info);
    if (info.isValid && info.filename) {
      setFilename(info.filename);
      if (info.rawSizeNum) {
        setTotalBytes(info.rawSizeNum);
      }
    }
  };

  // Handle Parse button click
  const handleParse = () => {
    if (!url.trim()) {
      addLog('WARN', '解析失败: 请先输入有效的 GitHub 链接');
      showToast('请先输入 GitHub 链接');
      return;
    }
    addLog('INFO', `正在解析链接: ${url}`);
    const info = parseGitHubUrl(url);
    setParsedInfo(info);

    if (info.isValid) {
      addLog('SUCCESS', `解析成功 [${info.typeLabel}] 文件名: ${info.filename} (预估: ${info.estimatedSize})`);
      setFilename(info.filename);
      if (info.rawSizeNum) {
        setTotalBytes(info.rawSizeNum);
      }
      showToast(`已成功解析: ${info.filename}`);
    } else {
      addLog('ERROR', '未能识别该链接的 GitHub 资源结构，请检查链接完整性');
      showToast('链接格式未识别');
    }
  };

  // Handle Paste from clipboard
  const handlePaste = async () => {
    try {
      if (navigator.clipboard && navigator.clipboard.readText) {
        const text = await navigator.clipboard.readText();
        if (text) {
          handleUrlChange(text);
          addLog('INFO', `已从剪贴板读取内容: ${text.slice(0, 45)}...`);
          showToast('已从剪贴板粘贴');
          const info = parseGitHubUrl(text);
          if (info.isValid) {
            handleParse();
          }
        }
      } else {
        showToast('浏览器限制访问剪贴板，请使用 Ctrl+V 粘贴');
      }
    } catch {
      showToast('请手动在输入框中使用 Ctrl+V 粘贴');
    }
  };

  // Select a preset link
  const handleSelectPreset = (presetUrl: string) => {
    setUrl(presetUrl);
    handleUrlChange(presetUrl);
    addLog('INFO', `载入示例预设: ${presetUrl.split('/').pop()}`);
  };

  // Select an asset from release
  const handleSelectAsset = (assetName: string, downloadUrl: string, size: string) => {
    setFilename(assetName);
    setUrl(downloadUrl);
    handleUrlChange(downloadUrl);
    addLog('INFO', `切换下载资产: ${assetName} (${size})`);
  };

  // Auto detect Clash Proxy Port
  const handleAutoDetectPort = () => {
    setProxySettings((prev) => ({ ...prev, status: 'checking' }));
    addLog('INFO', '正在扫描本地 Clash / V2Ray 代理服务端口 (7890, 7891, 10808, 10809)...');

    setTimeout(() => {
      const detectedPort = 7890;
      const latency = Math.floor(Math.random() * 20) + 38; // 38-58ms
      setProxySettings({
        port: detectedPort,
        protocol: 'HTTP',
        status: 'connected',
        latencyMs: latency,
        message: 'Clash 代理连接正常',
      });
      addLog('SUCCESS', `端口探测成功: 127.0.0.1:${detectedPort} 响应正常 (往返延时: ${latency}ms)`);
      showToast(`检测到 Clash 代理: 端口 ${detectedPort} (${latency}ms)`);
    }, 850);
  };

  // Test connection
  const handleTestConnection = () => {
    setProxySettings((prev) => ({ ...prev, status: 'checking' }));
    addLog('INFO', `正在测试端口 ${proxySettings.port} 的代理连接...`);

    setTimeout(() => {
      const isOk = proxySettings.port >= 1000 && proxySettings.port <= 65535;
      if (isOk) {
        const latency = Math.floor(Math.random() * 25) + 35;
        setProxySettings((prev) => ({
          ...prev,
          status: 'connected',
          latencyMs: latency,
          message: '代理连接正常',
        }));
        addLog('SUCCESS', `端口 ${proxySettings.port} 握手成功，GitHub API 连通性测试通过 (${latency}ms)`);
        showToast(`测试连接成功: ${latency}ms`);
      } else {
        setProxySettings((prev) => ({
          ...prev,
          status: 'error',
          latencyMs: null,
          message: '端口未响应',
        }));
        addLog('ERROR', `代理连接失败 (端口 ${proxySettings.port})，请检查 Clash 核心是否运行`);
        showToast('连接失败，请确认 Clash 正在运行');
      }
    }, 750);
  };

  // Select acceleration engine
  const handleSelectEngine = (engine: AccelerationEngine) => {
    setSelectedEngine(engine);
    const engineMap: Record<AccelerationEngine, string> = {
      clash: 'Clash 本地代理',
      ghproxy: 'GHProxy 镜像节点',
      kkgithub: 'KKGitHub 节点',
      moeyy: 'Moeyy 加速源',
      direct: '官方直连',
    };
    addLog('PROXY', `切换加速模式为: ${engineMap[engine]}`);
  };

  // Download Simulation Engine
  const downloadTimerRef = useRef<NodeJS.Timeout | null>(null);

  const handleStartDownload = () => {
    if (!url.trim()) {
      addLog('WARN', '下载未启动: 请先提供下载链接');
      showToast('请先输入 GitHub 链接');
      return;
    }

    const currentTotal = totalBytes > 0 ? totalBytes : 88713000;
    const finalAcceleratedUrl = buildAcceleratedUrl(url, selectedEngine);

    setDownloadStatus('downloading');
    setProgress(0);
    setBytesDownloaded(0);
    setSpeedHistory([]);

    addLog('INFO', `准备下载任务: ${filename}`);
    addLog('INFO', `目标路径: ${savePath}\\${filename}`);
    addLog('PROXY', `启用引擎: ${selectedEngine.toUpperCase()} · 并发线程数: ${threads}`);
    addLog('INFO', `开始建立 HTTP/2 分块传输通道...`);

    let currentBytes = 0;
    const startTime = Date.now();

    if (downloadTimerRef.current) clearInterval(downloadTimerRef.current);

    downloadTimerRef.current = setInterval(() => {
      // Generate realistic dynamic speed: ~15MB/s to ~28MB/s with slight variation
      const baseSpeed = 18 * 1024 * 1024;
      const variation = (Math.random() - 0.45) * 6 * 1024 * 1024;
      const currentSpeed = Math.max(3 * 1024 * 1024, baseSpeed + variation);

      // Increase bytes per tick (200ms tick)
      const chunk = (currentSpeed * 0.2);
      currentBytes += chunk;

      if (currentBytes >= currentTotal) {
        currentBytes = currentTotal;
        setProgress(100);
        setBytesDownloaded(currentTotal);
        setSpeedBps(0);
        setEtaSeconds(0);
        setDownloadStatus('completed');
        if (downloadTimerRef.current) clearInterval(downloadTimerRef.current);

        const durationSec = ((Date.now() - startTime) / 1000).toFixed(1);
        addLog('SUCCESS', `下载完成！共传输 ${(currentTotal / (1024 * 1024)).toFixed(1)} MB，耗时 ${durationSec} 秒`);
        addLog('INFO', `文件已保存至: ${savePath}\\${filename}`);
        playFinishSound();
        showToast('下载完成！');

        // Append to history
        const newTask: DownloadTask = {
          id: `task-${Date.now()}`,
          url,
          acceleratedUrl: finalAcceleratedUrl,
          filename,
          savePath,
          status: 'completed',
          progress: 100,
          speedBps: currentSpeed,
          bytesDownloaded: currentTotal,
          totalBytes: currentTotal,
          etaSeconds: 0,
          threads,
          createdAt: startTime,
          completedAt: Date.now(),
        };
        setTaskHistory((prev) => [newTask, ...prev]);

        // Auto trigger open folder log if checked
        if (openFolderAfterDownload) {
          addLog('INFO', `[自动操作] 已定位目标目录: ${savePath}`);
        }
      } else {
        const pct = (currentBytes / currentTotal) * 100;
        const remainingBytes = currentTotal - currentBytes;
        const eta = remainingBytes / currentSpeed;

        setProgress(pct);
        setBytesDownloaded(currentBytes);
        setSpeedBps(currentSpeed);
        setEtaSeconds(eta);
        setSpeedHistory((prev) => [...prev.slice(-19), currentSpeed]);
      }
    }, 200);
  };

  const handlePauseDownload = () => {
    if (downloadTimerRef.current) clearInterval(downloadTimerRef.current);
    setDownloadStatus('paused');
    setSpeedBps(0);
    addLog('WARN', '下载任务已暂停');
  };

  const handleResumeDownload = () => {
    setDownloadStatus('downloading');
    addLog('INFO', '正在断点续传下载...');

    const currentTotal = totalBytes > 0 ? totalBytes : 88713000;
    let currentBytes = bytesDownloaded;

    if (downloadTimerRef.current) clearInterval(downloadTimerRef.current);

    downloadTimerRef.current = setInterval(() => {
      const baseSpeed = 18 * 1024 * 1024;
      const variation = (Math.random() - 0.45) * 6 * 1024 * 1024;
      const currentSpeed = Math.max(3 * 1024 * 1024, baseSpeed + variation);
      const chunk = (currentSpeed * 0.2);
      currentBytes += chunk;

      if (currentBytes >= currentTotal) {
        currentBytes = currentTotal;
        setProgress(100);
        setBytesDownloaded(currentTotal);
        setSpeedBps(0);
        setEtaSeconds(0);
        setDownloadStatus('completed');
        if (downloadTimerRef.current) clearInterval(downloadTimerRef.current);
        addLog('SUCCESS', `下载完成！文件已保存至: ${savePath}\\${filename}`);
        playFinishSound();
        showToast('下载完成！');
      } else {
        const pct = (currentBytes / currentTotal) * 100;
        const remainingBytes = currentTotal - currentBytes;
        const eta = remainingBytes / currentSpeed;

        setProgress(pct);
        setBytesDownloaded(currentBytes);
        setSpeedBps(currentSpeed);
        setEtaSeconds(eta);
        setSpeedHistory((prev) => [...prev.slice(-19), currentSpeed]);
      }
    }, 200);
  };

  const handleCancelDownload = () => {
    if (downloadTimerRef.current) clearInterval(downloadTimerRef.current);
    setDownloadStatus('idle');
    setProgress(0);
    setBytesDownloaded(0);
    setSpeedBps(0);
    setEtaSeconds(0);
    setSpeedHistory([]);
    addLog('WARN', '下载任务已取消，临时缓存已清除');
    showToast('下载已取消');
  };

  const handleOpenFolder = () => {
    addLog('INFO', `已在资源管理器中定位: ${savePath}\\${filename}`);
    showToast(`已打开目录: ${savePath}`);
  };

  const handleRedownload = () => {
    handleStartDownload();
  };

  const handleDownloadDirectly = () => {
    const finalAcceleratedUrl = buildAcceleratedUrl(url, selectedEngine);
    addLog('INFO', `启动真实直链下载任务: ${filename}`);
    addLog('PROXY', `目标下载地址 (${selectedEngine.toUpperCase()}): ${finalAcceleratedUrl}`);

    const link = document.createElement('a');
    link.href = finalAcceleratedUrl;
    link.download = filename;
    link.target = '_blank';
    link.rel = 'noopener noreferrer';
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);

    showToast(`已触发 ${filename} 浏览器直链加速下载`);
    addLog('SUCCESS', `浏览器已接管下载请求: ${filename}`);
  };

  // Cleanup timers on unmount
  useEffect(() => {
    return () => {
      if (downloadTimerRef.current) clearInterval(downloadTimerRef.current);
    };
  }, []);

  const getEngineName = () => {
    switch (selectedEngine) {
      case 'clash':
        return 'Clash 本地代理';
      case 'ghproxy':
        return 'GHProxy 镜像';
      case 'kkgithub':
        return 'KKGitHub 节点';
      case 'moeyy':
        return 'Moeyy 镜像';
      default:
        return '官方直连';
    }
  };

  return (
    <div className="min-h-screen bg-neutral-200 dark:bg-neutral-950 p-2 sm:p-5 flex flex-col items-center justify-center font-sans antialiased selection:bg-blue-500/20 selection:text-blue-600 transition-colors duration-200">
      {/* Toast Notification */}
      {toastMessage && (
        <div className="fixed top-4 left-1/2 -translate-x-1/2 z-50 bg-neutral-900/90 dark:bg-white/95 text-white dark:text-neutral-900 px-4 py-2 rounded-lg text-xs font-medium shadow-lg backdrop-blur-xs flex items-center gap-2 animate-in fade-in slide-in-from-top-2 duration-150">
          <Info className="w-3.5 h-3.5 text-blue-400 dark:text-blue-600 shrink-0" />
          <span>{toastMessage}</span>
        </div>
      )}

      {/* Main Desktop Window Container */}
      <div 
        className={`w-full bg-neutral-100 dark:bg-neutral-925 dark:bg-[#121417] rounded-2xl border border-neutral-300/80 dark:border-neutral-800 shadow-2xl overflow-hidden flex flex-col transition-all duration-300 ${
          isMaximized 
            ? 'fixed inset-0 rounded-none z-40 max-w-none h-full' 
            : 'max-w-5xl max-h-[92vh] h-auto my-auto ring-1 ring-black/5 dark:ring-white/5'
        }`}
      >
        {/* Modern Window Title Bar */}
        <WindowHeader
          isDark={isDark}
          onToggleTheme={() => setIsDark(!isDark)}
          onOpenSettings={() => setIsSettingsOpen(true)}
          onOpenHistory={() => setIsHistoryOpen(true)}
          historyCount={taskHistory.length}
          engineName={getEngineName()}
          proxyStatus={proxySettings.status}
          proxyPort={proxySettings.port}
        />

        {/* Window Content Viewport (Two Columns, Zero Window Scroll) */}
        <div className="flex-1 p-3 sm:p-4 overflow-y-auto lg:overflow-hidden">
          <div className="grid grid-cols-1 lg:grid-cols-12 gap-3 h-full">
            {/* Left Column: Input & Configuration Panel (6 cols) */}
            <div className="lg:col-span-6 flex flex-col gap-3">
              {/* Section 1: GitHub URL Input & Parsing */}
              <UrlInputSection
                url={url}
                onUrlChange={handleUrlChange}
                onParse={handleParse}
                onPaste={handlePaste}
                parsedInfo={parsedInfo}
                onSelectPreset={handleSelectPreset}
                onSelectAsset={handleSelectAsset}
              />

              {/* Section 2: Save Directory & Filename */}
              <SaveLocationSection
                savePath={savePath}
                onSavePathChange={(path) => {
                  setSavePath(path);
                  addLog('INFO', `修改保存路径: ${path}`);
                }}
                filename={filename}
                onFilenameChange={setFilename}
                openFolderAfterDownload={openFolderAfterDownload}
                onToggleOpenFolder={setOpenFolderAfterDownload}
                onBrowseFolder={() => setIsFolderPickerOpen(true)}
              />

              {/* Section 3: Clash Proxy & Acceleration Node Setup */}
              <ProxyConfigSection
                proxySettings={proxySettings}
                onPortChange={(port) => {
                  setProxySettings((prev) => ({ ...prev, port }));
                  addLog('INFO', `修改代理端口为: ${port}`);
                }}
                onAutoDetect={handleAutoDetectPort}
                onTestConnection={handleTestConnection}
                selectedEngine={selectedEngine}
                onSelectEngine={handleSelectEngine}
              />
            </div>

            {/* Right Column: Execution & Telemetry Terminal Panel (6 cols) */}
            <div className="lg:col-span-6 flex flex-col gap-3 min-h-0">
              {/* Section 4: Download Progress & Controls */}
              <DownloadProgressSection
                status={downloadStatus}
                progress={progress}
                speedBps={speedBps}
                bytesDownloaded={bytesDownloaded}
                totalBytes={totalBytes}
                etaSeconds={etaSeconds}
                threads={threads}
                speedHistory={speedHistory}
                onStart={handleStartDownload}
                onPause={handlePauseDownload}
                onResume={handleResumeDownload}
                onCancel={handleCancelDownload}
                onOpenFolder={handleOpenFolder}
                onRedownload={handleRedownload}
                onDownloadDirectly={handleDownloadDirectly}
              />

              {/* Section 5: Real-time Runtime Logs Console (Flex-1 to fill height) */}
              <LogTerminalSection
                logs={logs}
                onClearLogs={() => {
                  setLogs([]);
                  addLog('INFO', '日志已清空');
                }}
              />
            </div>
          </div>
        </div>

        {/* Window Status Footer */}
        <footer className="h-7 bg-white dark:bg-neutral-900 border-t border-neutral-200 dark:border-neutral-800 px-4 flex items-center justify-between text-[11px] text-neutral-500 dark:text-neutral-400 select-none shrink-0 font-mono">
          <div className="flex items-center gap-3">
            <span>存储: {savePath.split('\\').pop() || 'dist'}</span>
            <span className="text-neutral-300 dark:text-neutral-700">|</span>
            <span>引擎: {selectedEngine.toUpperCase()}</span>
            <span className="text-neutral-300 dark:text-neutral-700">|</span>
            <span>代理: 127.0.0.1:{proxySettings.port}</span>
          </div>

          <div className="flex items-center gap-2">
            <span>分块传输: {threads}T</span>
            <span className="text-neutral-300 dark:text-neutral-700">|</span>
            <button
              onClick={() => setIsMaximized(!isMaximized)}
              className="hover:text-neutral-800 dark:hover:text-neutral-200 cursor-pointer flex items-center gap-1"
              title={isMaximized ? "恢复窗口模式" : "全屏沉浸模式"}
            >
              {isMaximized ? <Minimize2 className="w-3 h-3" /> : <Maximize2 className="w-3 h-3" />}
              <span className="hidden sm:inline">{isMaximized ? '还原' : '全屏'}</span>
            </button>
          </div>
        </footer>
      </div>

      {/* Modals */}
      <HistoryModal
        isOpen={isHistoryOpen}
        onClose={() => setIsHistoryOpen(false)}
        history={taskHistory}
        onClearHistory={() => {
          setTaskHistory([]);
          addLog('INFO', '已清空任务历史记录');
        }}
        onRedownload={(task) => {
          setUrl(task.url);
          setFilename(task.filename);
          setSavePath(task.savePath);
          setIsHistoryOpen(false);
          handleStartDownload();
        }}
      />

      <SettingsModal
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
        threads={threads}
        onChangeThreads={(t) => {
          setThreads(t);
          addLog('INFO', `并发线程调整为: ${t} 线程`);
        }}
        autoClipboard={autoClipboard}
        onToggleAutoClipboard={setAutoClipboard}
        soundEnabled={soundEnabled}
        onToggleSound={setSoundEnabled}
        timeoutSeconds={timeoutSeconds}
        onChangeTimeout={setTimeoutSeconds}
      />

      <FolderPickerModal
        isOpen={isFolderPickerOpen}
        onClose={() => setIsFolderPickerOpen(false)}
        currentPath={savePath}
        onSelectPath={(newPath) => {
          setSavePath(newPath);
          addLog('INFO', `已选择保存目录: ${newPath}`);
        }}
      />
    </div>
  );
}
