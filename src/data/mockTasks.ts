// 浏览器 dev 模式的演示数据（对齐设计稿 initialTasks.ts；Tauri 真实模式下不使用）
import type { DownloadTask, TaskSegment, PeerInfo, MirrorSource } from '../types';

function createSegments(totalBytes: number, downloadedBytes: number, count = 16): TaskSegment[] {
  const chunkSize = Math.floor(totalBytes / count);
  const segments: TaskSegment[] = [];
  for (let i = 0; i < count; i++) {
    const startByte = i * chunkSize;
    const endByte = i === count - 1 ? totalBytes : (i + 1) * chunkSize;
    const segmentTotal = endByte - startByte;
    let segDownloaded = 0;
    if (downloadedBytes >= endByte) segDownloaded = segmentTotal;
    else if (downloadedBytes > startByte) segDownloaded = downloadedBytes - startByte;
    const isComplete = segDownloaded >= segmentTotal;
    const isActivelyDownloading = !isComplete && segDownloaded > 0;
    const isStalled = !isComplete && i === 7 && isActivelyDownloading;
    segments.push({
      id: i + 1,
      startByte,
      endByte,
      downloadedBytes: segDownloaded,
      speed: isComplete ? 0 : isStalled ? 12000 : Math.floor(Math.random() * 2800000 + 1200000),
      latencyMs: isStalled ? 840 : Math.floor(Math.random() * 35 + 14),
      status: isComplete ? 'completed' : isStalled ? 'stalled' : isActivelyDownloading ? 'active' : 'idle',
      workerIp: `104.21.${Math.floor(Math.random() * 80 + 10)}.${Math.floor(Math.random() * 200 + 10)}`,
    });
  }
  return segments;
}

const mockPeers: PeerInfo[] = [
  { id: 'p1', ip: '118.25.90.12', client: 'qBittorrent/4.6.3', downloadSpeed: 1840000, uploadSpeed: 420000, progress: 98.4, country: 'CN' },
  { id: 'p2', ip: '45.76.104.88', client: 'Transmission/4.0.5', downloadSpeed: 950000, uploadSpeed: 180000, progress: 84.2, country: 'JP' },
  { id: 'p3', ip: '198.51.100.4', client: 'Aria2/1.37.0', downloadSpeed: 1200000, uploadSpeed: 310000, progress: 100, country: 'US' },
];

const mockMirrors: MirrorSource[] = [
  { id: 'm1', name: '微软 Azure 官方 CDN (上海节点)', url: 'https://software.download.prss.microsoft.com', latencyMs: 16, speed: 18400000, status: 'active', isFastest: true },
  { id: 'm2', name: '阿里云开源镜像站 (杭州 BGP)', url: 'https://mirrors.aliyun.com/ms-system', latencyMs: 24, speed: 12200000, status: 'active' },
  { id: 'm3', name: '清华大学 TUNA 镜像站 (北京 IPv6)', url: 'https://mirrors.tuna.tsinghua.edu.cn', latencyMs: 31, speed: 8500000, status: 'active' },
];

export const MOCK_TASKS: DownloadTask[] = [
  {
    id: 'task-1',
    name: 'Windows11_24H2_Chinese_x64.iso',
    url: 'https://software.download.prss.microsoft.com/dbazure/Windows11_24H2_Chinese_x64.iso',
    category: 'archive',
    status: 'downloading',
    totalSize: 5976883200,
    downloadedSize: 3824000000,
    speed: 24800000,
    threads: 16,
    segments: createSegments(5976883200, 3824000000, 16),
    createdAt: Date.now() - 1000 * 180,
    savePath: 'D:\\Downloads\\System_ISOs',
    priority: 'high',
    supportsRange: true,
    kind: 'http',
    useClash: true,
  },
  {
    id: 'task-2',
    name: 'Blender-4.2.1-windows-x64.msi',
    url: 'https://download.blender.org/release/Blender4.2/blender-4.2.1-windows-x64.msi',
    category: 'software',
    status: 'downloading',
    totalSize: 420478976,
    downloadedSize: 374226288,
    speed: 14200000,
    threads: 8,
    segments: createSegments(420478976, 374226288, 8),
    createdAt: Date.now() - 1000 * 45,
    savePath: 'C:\\Users\\Administrator\\Downloads\\Programs',
    supportsRange: true,
    kind: 'http',
  },
  {
    id: 'task-3',
    name: 'Cosmos_DeepSpace_Dataset_Vol4.torrent',
    url: 'magnet:?xt=urn:btih:8f3c7e46929a0b12c5890d234e8f1920abcdef12&dn=Cosmos_DeepSpace_Dataset',
    category: 'torrent',
    status: 'downloading',
    totalSize: 15461882880,
    downloadedSize: 6648609638,
    speed: 8400000,
    uploadSpeed: 2100000,
    threads: 32,
    segments: createSegments(15461882880, 6648609638, 16),
    createdAt: Date.now() - 1000 * 600,
    savePath: 'D:\\Torrents\\Complete',
    isTorrent: true,
    priority: 'normal',
    supportsRange: true,
    kind: 'bt',
  },
  {
    id: 'task-4',
    name: 'Tears_of_Steel_4K_HDR_60fps.mkv',
    url: 'https://media.xiph.org/tearsofsteel/tears_of_steel_4k_hdr.mkv',
    category: 'video',
    status: 'completed',
    totalSize: 2845600000,
    downloadedSize: 2845600000,
    speed: 0,
    threads: 16,
    segments: createSegments(2845600000, 2845600000, 16),
    createdAt: Date.now() - 1000 * 3600 * 2,
    completedAt: Date.now() - 1000 * 3600 * 1.8,
    savePath: 'D:\\Videos\\4K_Collection',
    supportsRange: true,
    kind: 'http',
  },
  {
    id: 'task-5',
    name: 'node-v22.11.0-x64.msi',
    url: 'https://nodejs.org/dist/v22.11.0/node-v22.11.0-x64.msi',
    category: 'software',
    status: 'completed',
    totalSize: 52428800,
    downloadedSize: 52428800,
    speed: 0,
    threads: 8,
    segments: createSegments(52428800, 52428800, 8),
    createdAt: Date.now() - 1000 * 3600 * 5,
    completedAt: Date.now() - 1000 * 3600 * 4.95,
    savePath: 'C:\\Users\\Administrator\\Downloads',
    supportsRange: true,
    kind: 'http',
  },
  {
    id: 'task-6',
    name: 'VSCodeSetup-x64-1.95.1.exe',
    url: 'https://vscode.download.prss.microsoft.com/dbazure/download/stable/VSCodeSetup-x64-1.95.1.exe',
    category: 'software',
    status: 'paused',
    totalSize: 734003200,
    downloadedSize: 381681664,
    speed: 0,
    threads: 8,
    segments: createSegments(734003200, 381681664, 8),
    createdAt: Date.now() - 1000 * 3600,
    savePath: 'C:\\Users\\Administrator\\Downloads\\Programs',
    supportsRange: true,
    kind: 'http',
  },
  {
    id: 'task-7',
    name: 'DirectX_12_Ultimate_Developer_Guide.pdf',
    url: 'https://learn.microsoft.com/windows/win32/direct3d12/directx-12-manual.pdf',
    category: 'document',
    status: 'completed',
    totalSize: 18874368,
    downloadedSize: 18874368,
    speed: 0,
    threads: 4,
    segments: createSegments(18874368, 18874368, 4),
    createdAt: Date.now() - 1000 * 86400,
    completedAt: Date.now() - 1000 * 86350,
    savePath: 'D:\\Documents\\DevGuides',
    priority: 'low',
    supportsRange: true,
    kind: 'http',
  },
  {
    id: 'task-8',
    name: 'Unreal_Engine_5.5_Sample_Vehicle_Rig.zip',
    url: 'https://epicgames.download/ue5/samples/vehicle_pack_v2.zip',
    category: 'archive',
    status: 'error',
    totalSize: 8589934592,
    downloadedSize: 1288490188,
    speed: 0,
    threads: 16,
    segments: createSegments(8589934592, 1288490188, 16),
    createdAt: Date.now() - 1000 * 7200,
    savePath: 'D:\\Downloads',
    errorMessage: 'HTTP 403 Forbidden: 服务器拒绝连接或令牌超时',
    supportsRange: false,
    kind: 'http',
  },
];

export const MOCK_PEERS = mockPeers;
export const MOCK_MIRRORS = mockMirrors;
