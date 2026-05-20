'use client';

import { useState, useEffect, useRef } from 'react';
import { useAuth, fileApi, userApi } from '@/lib/api';
import { FileInfo, QuotaInfo } from '@/types';
import { useRouter } from 'next/navigation';
import { 
  Folder, File, Upload, Search, Plus, MoreVertical, 
  Trash2, RotateCcw, Link as LinkIcon,
  Settings, Bell, Moon, Check, Download, X, Menu,
  ChevronRight, ChevronDown, Zap, Image as ImageIcon, Video, Music, FileText, Archive
} from 'lucide-react';
import toast from 'react-hot-toast';
import FilePreviewModal from '@/components/FilePreviewModal';

// 文件分类类型
type FileType = 'all' | 'image' | 'video' | 'audio' | 'document' | 'archive' | 'other';

export default function DashboardPage() {
  const { user, logout, isAuthenticated } = useAuth();
  const router = useRouter();
  const [files, setFiles] = useState<FileInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [quota, setQuota] = useState<QuotaInfo | null>(null);
  const [parentId, setParentId] = useState<string | undefined>(undefined);
  const [searchQuery, setSearchQuery] = useState('');
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [activeTab, setActiveTab] = useState<FileType>('all');
  const [sortBy, setSortBy] = useState<'name' | 'size' | 'date'>('date');
  const [sortOrder, setSortOrder] = useState<'asc' | 'desc'>('desc');
  const [selectedFiles, setSelectedFiles] = useState<string[]>([]);
  const [uploading, setUploading] = useState(false);
  const [uploadProgress, setUploadProgress] = useState(0);
  const [previewFile, setPreviewFile] = useState<FileInfo | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [darkMode, setDarkMode] = useState(false);

  useEffect(() => {
    if (!isAuthenticated) {
      router.push('/login');
      return;
    }
    loadFiles();
    loadQuota();
  }, [parentId, activeTab, isAuthenticated, router]);

  const loadFiles = async () => {
    try {
      setLoading(true);
      const response = await fileApi.list({ parent_id: parentId, page_size: 100 });
      let filteredFiles = response.data.files;

      // 按文件类型筛选
      if (activeTab !== 'all') {
        filteredFiles = filteredFiles.filter(file => {
          if (file.type === 'folder') return false;
          const mime = file.mime_type || '';
          switch (activeTab) {
            case 'image': return mime.startsWith('image/');
            case 'video': return mime.startsWith('video/');
            case 'audio': return mime.startsWith('audio/');
            case 'document': return mime.includes('document') || mime.includes('word') || mime.includes('pdf') || mime.includes('text');
            case 'archive': return mime.includes('zip') || mime.includes('rar') || mime.includes('7z') || mime.includes('tar');
            case 'other': return true;
            default: return true;
          }
        });
      }

      // 排序
      filteredFiles.sort((a, b) => {
        let comparison = 0;
        if (sortBy === 'name') {
          comparison = a.name.localeCompare(b.name);
        } else if (sortBy === 'size') {
          comparison = a.size - b.size;
        } else if (sortBy === 'date') {
          comparison = new Date(a.created_at).getTime() - new Date(b.created_at).getTime();
        }
        return sortOrder === 'asc' ? comparison : -comparison;
      });

      setFiles(filteredFiles);
    } catch (error) {
      toast.error('加载文件失败');
    } finally {
      setLoading(false);
    }
  };

  const loadQuota = async () => {
    try {
      const response = await userApi.getQuota();
      setQuota(response.data);
    } catch (error) {
      console.error('加载配额失败', error);
    }
  };

  const handleCreateFolder = async () => {
    const name = prompt('请输入文件夹名称:');
    if (!name) return;

    try {
      await fileApi.createFolder({ name, parent_id: parentId });
      toast.success('文件夹创建成功');
      loadFiles();
    } catch (error: any) {
      toast.error(error.response?.data?.error || '创建文件夹失败');
    }
  };

  const handleUploadFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    setUploading(true);
    setUploadProgress(0);

    try {
      if (file.size < 10 * 1024 * 1024) {
        await fileApi.uploadSimple(file);
        toast.success('文件上传成功');
        loadFiles();
      } else {
        await handleLargeFileUpload(file);
      }
    } catch (error: any) {
      toast.error(error.response?.data?.error || '上传失败');
    } finally {
      setUploading(false);
      setUploadProgress(0);
      if (fileInputRef.current) {
        fileInputRef.current.value = '';
      }
    }
  };

  const handleLargeFileUpload = async (file: File) => {
    const session = await fileApi.createUploadSession({
      file_name: file.name,
      file_size: file.size,
      chunk_size: 5 * 1024 * 1024,
      mime_type: file.type,
    });

    const chunkSize = session.data.chunk_size;
    const totalChunks = Math.ceil(file.size / chunkSize);

    for (let i = 0; i < totalChunks; i++) {
      const start = i * chunkSize;
      const end = Math.min(start + chunkSize, file.size);
      const chunk = file.slice(start, end);

      await fileApi.uploadChunk(session.data.id, i, chunk);
      setUploadProgress(((i + 1) / totalChunks) * 100);
    }

    const fileHash = await calculateFileHash(file);
    await fileApi.completeUploadSession(session.data.id, fileHash);
    
    toast.success('文件上传成功');
    loadFiles();
  };

  const calculateFileHash = async (file: File): Promise<string> => {
    return crypto.randomUUID();
  };

  const handleDeleteFile = async (file: FileInfo) => {
    if (!confirm(`确定要删除 "${file.name}" 吗？`)) return;

    try {
      await fileApi.delete(file.id);
      toast.success('删除成功');
      loadFiles();
    } catch (error: any) {
      toast.error(error.response?.data?.error || '删除失败');
    }
  };

  const handleDownloadFile = async (file: FileInfo) => {
    try {
      const response = await fileApi.download(file.id);
      const url = window.URL.createObjectURL(new Blob([response.data]));
      const link = document.createElement('a');
      link.href = url;
      link.setAttribute('download', file.name);
      document.body.appendChild(link);
      link.click();
      link.remove();
      window.URL.revokeObjectURL(url);
      toast.success('下载已开始');
    } catch (error: any) {
      toast.error(error.response?.data?.error || '下载失败');
    }
  };

  const handleNavigateToFolder = (file: FileInfo) => {
    if (file.type === 'folder') {
      setParentId(file.id);
    }
  };

  const handleSearch = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!searchQuery.trim()) {
      setParentId(undefined);
      loadFiles();
      return;
    }

    try {
      const response = await fileApi.search({ q: searchQuery, page_size: 100 });
      setFiles(response.data.files);
    } catch (error) {
      toast.error('搜索失败');
    }
  };

  const handleClearSearch = () => {
    setSearchQuery('');
    setParentId(undefined);
    loadFiles();
  };

  const handleSelectFile = (fileId: string) => {
    if (selectedFiles.includes(fileId)) {
      setSelectedFiles(selectedFiles.filter(id => id !== fileId));
    } else {
      setSelectedFiles([...selectedFiles, fileId]);
    }
  };

  const handleSelectAll = () => {
    if (selectedFiles.length === files.length) {
      setSelectedFiles([]);
    } else {
      setSelectedFiles(files.map(f => f.id));
    }
  };

  const getFileIcon = (file: FileInfo) => {
    if (file.type === 'folder') {
      return <Folder className="w-5 h-5 text-yellow-500" />;
    }

    const mime = file.mime_type || '';
    if (mime.startsWith('image/')) {
      return <ImageIcon className="w-5 h-5 text-blue-500" />;
    }
    if (mime.startsWith('video/')) {
      return <Video className="w-5 h-5 text-red-500" />;
    }
    if (mime.startsWith('audio/')) {
      return <Music className="w-5 h-5 text-purple-500" />;
    }
    if (mime.includes('document') || mime.includes('word') || mime.includes('pdf') || mime.includes('text')) {
      return <FileText className="w-5 h-5 text-green-500" />;
    }
    if (mime.includes('zip') || mime.includes('rar') || mime.includes('7z') || mime.includes('tar')) {
      return <Archive className="w-5 h-5 text-orange-500" />;
    }
    return <File className="w-5 h-5 text-gray-500" />;
  };

  if (!isAuthenticated) {
    return null;
  }

  return (
    <div className={`min-h-screen ${darkMode ? 'dark' : ''}`}>
      <div className="flex h-screen bg-gray-50">
        {/* 左侧导航栏 */}
        <div className={`${sidebarCollapsed ? 'w-16' : 'w-60'} bg-white border-r border-gray-200 flex flex-col transition-all duration-300`}>
          {/* Logo */}
          <div className="h-16 flex items-center justify-between px-4 border-b border-gray-100">
            {!sidebarCollapsed && (
              <div className="flex items-center space-x-2">
                <div className="w-8 h-8 bg-gradient-to-br from-blue-600 to-blue-400 rounded-lg flex items-center justify-center">
                  <Zap className="w-5 h-5 text-white" />
                </div>
                <span className="text-lg font-bold text-gray-900">个人笔记</span>
              </div>
            )}
            <button
              onClick={() => setSidebarCollapsed(!sidebarCollapsed)}
              className="p-1.5 rounded-lg hover:bg-gray-100 text-gray-500"
            >
              <Menu className="w-5 h-5" />
            </button>
          </div>

          {/* 导航菜单 */}
          <div className="flex-1 overflow-y-auto py-4">
            <nav className="space-y-1 px-2">
              <button
                onClick={() => { setActiveTab('all'); setParentId(undefined); }}
                className={`w-full flex items-center space-x-3 px-3 py-2 rounded-lg text-sm font-medium transition-all ${
                  activeTab === 'all' && !parentId
                    ? 'bg-blue-50 text-blue-600'
                    : 'text-gray-600 hover:bg-gray-100'
                }`}
              >
                <Folder className="w-5 h-5" />
                {!sidebarCollapsed && <span>全部文件</span>}
                {(activeTab === 'all' && !parentId) && !sidebarCollapsed && (
                  <ChevronRight className="w-4 h-4 ml-auto" />
                )}
              </button>

              <button
                onClick={() => setActiveTab('image')}
                className={`w-full flex items-center space-x-3 px-3 py-2 rounded-lg text-sm font-medium transition-all ${
                  activeTab === 'image' ? 'bg-blue-50 text-blue-600' : 'text-gray-600 hover:bg-gray-100'
                }`}
              >
                <ImageIcon className="w-5 h-5" />
                {!sidebarCollapsed && <span>图片</span>}
              </button>

              <button
                onClick={() => setActiveTab('video')}
                className={`w-full flex items-center space-x-3 px-3 py-2 rounded-lg text-sm font-medium transition-all ${
                  activeTab === 'video' ? 'bg-blue-50 text-blue-600' : 'text-gray-600 hover:bg-gray-100'
                }`}
              >
                <Video className="w-5 h-5" />
                {!sidebarCollapsed && <span>视频</span>}
              </button>

              <button
                onClick={() => setActiveTab('audio')}
                className={`w-full flex items-center space-x-3 px-3 py-2 rounded-lg text-sm font-medium transition-all ${
                  activeTab === 'audio' ? 'bg-blue-50 text-blue-600' : 'text-gray-600 hover:bg-gray-100'
                }`}
              >
                <Music className="w-5 h-5" />
                {!sidebarCollapsed && <span>音频</span>}
              </button>

              <button
                onClick={() => setActiveTab('document')}
                className={`w-full flex items-center space-x-3 px-3 py-2 rounded-lg text-sm font-medium transition-all ${
                  activeTab === 'document' ? 'bg-blue-50 text-blue-600' : 'text-gray-600 hover:bg-gray-100'
                }`}
              >
                <FileText className="w-5 h-5" />
                {!sidebarCollapsed && <span>文档</span>}
              </button>

              <button
                onClick={() => setActiveTab('archive')}
                className={`w-full flex items-center space-x-3 px-3 py-2 rounded-lg text-sm font-medium transition-all ${
                  activeTab === 'archive' ? 'bg-blue-50 text-blue-600' : 'text-gray-600 hover:bg-gray-100'
                }`}
              >
                <Archive className="w-5 h-5" />
                {!sidebarCollapsed && <span>压缩包</span>}
              </button>

              <button
                onClick={() => setActiveTab('other')}
                className={`w-full flex items-center space-x-3 px-3 py-2 rounded-lg text-sm font-medium transition-all ${
                  activeTab === 'other' ? 'bg-blue-50 text-blue-600' : 'text-gray-600 hover:bg-gray-100'
                }`}
              >
                <File className="w-5 h-5" />
                {!sidebarCollapsed && <span>其他</span>}
              </button>

              <div className="border-t border-gray-100 my-2 pt-2">
                <button
                  onClick={() => setActiveTab('share')}
                  className={`w-full flex items-center space-x-3 px-3 py-2 rounded-lg text-sm font-medium transition-all ${
                    activeTab === 'share' ? 'bg-blue-50 text-blue-600' : 'text-gray-600 hover:bg-gray-100'
                  }`}
                >
                  <LinkIcon className="w-5 h-5" />
                  {!sidebarCollapsed && <span>分享管理</span>}
                </button>

                <button
                  onClick={() => setActiveTab('recycle')}
                  className={`w-full flex items-center space-x-3 px-3 py-2 rounded-lg text-sm font-medium transition-all ${
                    activeTab === 'recycle' ? 'bg-blue-50 text-blue-600' : 'text-gray-600 hover:bg-gray-100'
                  }`}
                >
                  <Trash2 className="w-5 h-5" />
                  {!sidebarCollapsed && <span>回收站</span>}
                </button>
              </div>

              <div className="border-t border-gray-100 my-2 pt-2">
                <button
                  className="w-full flex items-center space-x-3 px-3 py-2 rounded-lg text-sm font-medium text-gray-600 hover:bg-gray-100"
                >
                  <Bell className="w-5 h-5" />
                  {!sidebarCollapsed && <span>云盘公告</span>}
                </button>

                <button
                  className="w-full flex items-center space-x-3 px-3 py-2 rounded-lg text-sm font-medium text-gray-600 hover:bg-gray-100"
                >
                  <div className="w-5 h-5 bg-gradient-to-br from-blue-600 to-blue-400 rounded-full flex items-center justify-center">
                    <span className="text-white text-xs font-medium">{user?.username?.charAt(0).toUpperCase()}</span>
                  </div>
                  {!sidebarCollapsed && <span>个人中心</span>}
                </button>
              </div>
            </nav>
          </div>

          {/* 配额显示 */}
          {!sidebarCollapsed && quota && (
            <div className="p-4 border-t border-gray-100">
              <div className="bg-gray-50 rounded-lg p-3">
                <div className="flex justify-between text-sm mb-1">
                  <span className="text-gray-600">我的配额</span>
                  <span className="font-medium">{formatFileSize(quota.used)} / {formatFileSize(quota.total)}</span>
                </div>
                <div className="w-full bg-gray-200 rounded-full h-1.5">
                  <div
                    className="bg-blue-600 h-1.5 rounded-full transition-all duration-300"
                    style={{ width: `${(quota.used / quota.total) * 100}%` }}
                  />
                </div>
                <div className="text-xs text-gray-500 mt-1">单文件 ≤ 10.00 GB</div>
              </div>
            </div>
          )}
        </div>

        {/* 主内容区 */}
        <div className="flex-1 flex flex-col overflow-hidden">
          {/* 顶部工具栏 */}
          <div className="h-16 bg-white border-b border-gray-200 flex items-center justify-between px-6">
            {/* 面包屑 */}
            <div className="flex items-center space-x-2 text-sm text-gray-600">
              <span className="text-gray-900 font-medium">我的文件</span>
              {parentId && (
                <>
                  <ChevronRight className="w-4 h-4" />
                  <span>{parentId}</span>
                </>
              )}
            </div>

            {/* 右侧工具栏 */}
            <div className="flex items-center space-x-3">
              <button 
                onClick={() => setDarkMode(!darkMode)}
                className="p-2 rounded-lg hover:bg-gray-100 text-gray-500"
              >
                <Moon className="w-5 h-5" />
              </button>
              <button className="p-2 rounded-lg hover:bg-gray-100 text-gray-500">
                <Bell className="w-5 h-5" />
              </button>
              <div className="flex items-center space-x-2 pl-3 border-l border-gray-200">
                <div className="w-8 h-8 bg-yellow-400 rounded-full flex items-center justify-center text-white font-medium text-sm">
                  {user?.username?.charAt(0).toUpperCase()}
                </div>
                <span className="text-sm text-gray-700">{user?.username}</span>
                <ChevronDown className="w-4 h-4 text-gray-400" />
              </div>
            </div>
          </div>

          {/* 内容区域 */}
          <div className="flex-1 overflow-y-auto p-6">
            {/* 操作栏 */}
            <div className="bg-white rounded-lg shadow-sm border border-gray-200 p-4 mb-6">
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-3">
                  <button
                    onClick={() => fileInputRef.current?.click()}
                    className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg text-sm font-medium flex items-center space-x-2 transition-all"
                  >
                    <Upload className="w-4 h-4" />
                    <span>上传</span>
                  </button>
                  <input
                    ref={fileInputRef}
                    type="file"
                    className="hidden"
                    onChange={handleUploadFile}
                    disabled={uploading}
                  />
                  <button
                    onClick={handleCreateFolder}
                    className="px-4 py-2 border border-gray-200 hover:bg-gray-50 text-gray-700 rounded-lg text-sm font-medium flex items-center space-x-2 transition-all"
                  >
                    <Folder className="w-4 h-4" />
                    <span>新建文件夹</span>
                  </button>
                  <button className="px-4 py-2 bg-orange-50 hover:bg-orange-100 text-orange-600 rounded-lg text-sm font-medium flex items-center space-x-2 transition-all">
                    <Trash2 className="w-4 h-4" />
                    <span>一键清理过期文件</span>
                  </button>
                </div>

                <div className="flex items-center space-x-3">
                  {/* 搜索框 */}
                  <div className="relative">
                    <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
                    <input
                      type="text"
                      placeholder="搜索云端文件"
                      value={searchQuery}
                      onChange={(e) => setSearchQuery(e.target.value)}
                      onKeyPress={(e) => e.key === 'Enter' && handleSearch(e)}
                      className="w-64 pl-10 pr-4 py-2 border border-gray-200 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
                    />
                  </div>

                  {/* 筛选下拉 */}
                  <select
                    value={activeTab}
                    onChange={(e) => setActiveTab(e.target.value as FileType)}
                    className="px-3 py-2 border border-gray-200 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                  >
                    <option value="all">全部文件</option>
                    <option value="image">图片</option>
                    <option value="video">视频</option>
                    <option value="audio">音频</option>
                    <option value="document">文档</option>
                    <option value="archive">压缩包</option>
                    <option value="other">其他</option>
                  </select>

                  {/* 排序下拉 */}
                  <select
                    value={`${sortBy}-${sortOrder}`}
                    onChange={(e) => {
                      const [newSortBy, newSortOrder] = e.target.value.split('-');
                      setSortBy(newSortBy as 'name' | 'size' | 'date');
                      setSortOrder(newSortOrder as 'asc' | 'desc');
                    }}
                    className="px-3 py-2 border border-gray-200 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                  >
                    <option value="date-desc">最新上传</option>
                    <option value="date-asc">最旧上传</option>
                    <option value="name-asc">名称 A-Z</option>
                    <option value="name-desc">名称 Z-A</option>
                    <option value="size-desc">大小 从大到小</option>
                    <option value="size-asc">大小 从小到大</option>
                  </select>

                  <button className="p-2 rounded-lg hover:bg-gray-100 text-gray-500">
                    <RotateCcw className="w-4 h-4" />
                  </button>
                  <button className="p-2 rounded-lg hover:bg-gray-100 text-gray-500">
                    <Settings className="w-4 h-4" />
                  </button>
                </div>
              </div>
            </div>

            {/* 文件列表 */}
            <div className="bg-white rounded-lg shadow-sm border border-gray-200 overflow-hidden">
              {/* 表头 */}
              <div className="bg-gray-50 border-b border-gray-200 px-4 py-3 flex items-center text-sm font-medium text-gray-600">
                <input
                  type="checkbox"
                  checked={selectedFiles.length === files.length && files.length > 0}
                  onChange={handleSelectAll}
                  className="w-4 h-4 text-blue-600 rounded focus:ring-blue-500"
                />
                <span className="flex-1 ml-4">文件名</span>
                <span className="w-24 text-center">大小</span>
                <span className="w-20 text-center">下载次数</span>
                <span className="w-40 text-center">修改时间</span>
                <span className="w-40 text-center">操作</span>
              </div>

              {/* 文件列表内容 */}
              <div className="divide-y divide-gray-100">
                {loading ? (
                  <div className="px-4 py-12 text-center text-gray-500">加载中...</div>
                ) : files.length === 0 ? (
                  <div className="px-4 py-12 text-center">
                    <div className="mb-4">
                      <Folder className="w-16 h-16 mx-auto text-gray-300" />
                    </div>
                    <p className="text-gray-500">暂无文件</p>
                    <p className="text-sm text-gray-400 mt-1">点击上方按钮上传文件或创建文件夹</p>
                  </div>
                ) : (
                  files.map((file) => (
                    <div
                      key={file.id}
                      className={`px-4 py-3 flex items-center hover:bg-gray-50 group cursor-pointer ${
                        selectedFiles.includes(file.id) ? 'bg-blue-50' : ''
                      }`}
                      onDoubleClick={() => handleNavigateToFolder(file)}
                    >
                      <input
                        type="checkbox"
                        checked={selectedFiles.includes(file.id)}
                        onChange={() => handleSelectFile(file.id)}
                        className="w-4 h-4 text-blue-600 rounded focus:ring-blue-500"
                      />
                      <div className="flex items-center space-x-3 ml-4 flex-1">
                        {getFileIcon(file)}
                        <div>
                          <div className="font-medium text-gray-900">{file.name}</div>
                          {file.type === 'file' && file.mime_type && (
                            <div className="text-xs text-gray-500">{file.mime_type}</div>
                          )}
                        </div>
                      </div>
                      <div className="w-24 text-center text-sm text-gray-600">
                        {file.type === 'file' ? formatFileSize(file.size) : '-'}
                      </div>
                      <div className="w-20 text-center text-sm text-gray-500">
                        0 次
                      </div>
                      <div className="w-40 text-center text-sm text-gray-500">
                        {new Date(file.created_at).toLocaleString('zh-CN')}
                      </div>
                      <div className="w-40 flex items-center justify-center space-x-2 opacity-0 group-hover:opacity-100 transition-opacity">
                        <button
                          onClick={(e) => { e.stopPropagation(); handleDownloadFile(file); }}
                          className="text-blue-600 hover:text-blue-700 text-sm"
                        >
                          下载
                        </button>
                        <button
                          onClick={(e) => { e.stopPropagation(); handleDeleteFile(file); }}
                          className="text-red-600 hover:text-red-700"
                        >
                          <Trash2 className="w-4 h-4" />
                        </button>
                        <button className="text-gray-400 hover:text-gray-600">
                          <MoreVertical className="w-4 h-4" />
                        </button>
                      </div>
                    </div>
                  ))
                )}
              </div>

              {/* 分页信息 */}
              {files.length > 0 && (
                <div className="px-4 py-3 border-t border-gray-100 text-center text-sm text-gray-500">
                  — 已到底部，共 {files.length} 项 —
                </div>
              )}
            </div>
          </div>
        </div>
      </div>

      {/* 上传进度条 */}
      {uploading && (
        <div className="fixed bottom-4 right-4 bg-white rounded-lg shadow-lg border border-gray-200 p-4 min-w-80">
          <div className="flex items-center justify-between mb-2">
            <span className="text-sm font-medium text-gray-700">上传中...</span>
            <span className="text-sm text-blue-600 font-medium">{uploadProgress.toFixed(1)}%</span>
          </div>
          <div className="w-full bg-gray-200 rounded-full h-2">
            <div
              className="bg-blue-600 h-2 rounded-full transition-all duration-300"
              style={{ width: `${uploadProgress}%` }}
            />
          </div>
        </div>
      )}

      {/* 文件预览模态框 */}
      {previewFile && (
        <FilePreviewModal
          file={previewFile}
          onClose={() => setPreviewFile(null)}
          onDownload={() => {
            handleDownloadFile(previewFile);
            setPreviewFile(null);
          }}
        />
      )}
    </div>
  );
}

function formatFileSize(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}
