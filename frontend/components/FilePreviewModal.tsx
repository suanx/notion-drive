'use client';

import { useState } from 'react';
import { FileInfo } from '@/types';
import { X, Download, Share2, Eye, File as FileIcon, Image as ImageIcon, Video, Music, FileText } from 'lucide-react';

interface FilePreviewModalProps {
  file: FileInfo;
  onClose: () => void;
  onDownload: () => void;
}

export default function FilePreviewModal({ file, onClose, onDownload }: FilePreviewModalProps) {
  const [loading, setLoading] = useState(true);

  const getFilePreview = () => {
    if (file.type === 'folder') {
      return (
        <div className="flex flex-col items-center justify-center h-full text-gray-500">
          <FileIcon className="w-24 h-24 mb-4 text-gray-300" />
          <p className="text-lg font-medium">文件夹</p>
          <p className="text-sm mt-2">文件夹无法预览</p>
        </div>
      );
    }

    const mime = file.mime_type || '';

    // 图片预览
    if (mime.startsWith('image/')) {
      return (
        <div className="flex items-center justify-center h-full bg-gray-900">
          <img
            src={`/api/v1/files/${file.id}/download`}
            alt={file.name}
            className="max-w-full max-h-full object-contain"
            onLoad={() => setLoading(false)}
          />
        </div>
      );
    }

    // 视频预览
    if (mime.startsWith('video/')) {
      return (
        <div className="flex items-center justify-center h-full bg-gray-900">
          <video
            src={`/api/v1/files/${file.id}/download`}
            controls
            className="max-w-full max-h-full"
            autoPlay
          />
        </div>
      );
    }

    // 音频预览
    if (mime.startsWith('audio/')) {
      return (
        <div className="flex flex-col items-center justify-center h-full bg-gray-900">
          <Music className="w-24 h-24 mb-4 text-gray-300" />
          <audio
            src={`/api/v1/files/${file.id}/download`}
            controls
            className="w-full max-w-md"
            autoPlay
          />
        </div>
      );
    }

    // PDF 预览
    if (mime.includes('pdf')) {
      return (
        <div className="flex items-center justify-center h-full bg-gray-900">
          <iframe
            src={`/api/v1/files/${file.id}/download`}
            className="w-full h-full"
            title={file.name}
          />
        </div>
      );
    }

    // 文本文件预览
    if (mime.includes('text') || mime.includes('json') || mime.includes('xml') || mime.includes('html')) {
      return (
        <div className="flex flex-col h-full bg-gray-900">
          <div className="flex-1 overflow-auto p-4">
            <pre className="text-sm text-gray-300 font-mono">
              [文本文件预览功能正在开发中...]
            </pre>
          </div>
        </div>
      );
    }

    // 其他文件类型
    return (
      <div className="flex flex-col items-center justify-center h-full text-gray-500">
        <FileIcon className="w-24 h-24 mb-4 text-gray-300" />
        <p className="text-lg font-medium">{file.name}</p>
        <p className="text-sm mt-2">该文件类型无法预览</p>
        <button
          onClick={onDownload}
          className="mt-4 px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg text-sm flex items-center space-x-2"
        >
          <Download className="w-4 h-4" />
          <span>下载文件</span>
        </button>
      </div>
    );
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onClick={onClose}>
      <div 
        className="bg-white rounded-lg w-full max-w-5xl h-[80vh] flex flex-col overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        {/* 头部 */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-gray-200">
          <div className="flex items-center space-x-3">
            {file.type === 'folder' ? (
              <FileIcon className="w-5 h-5 text-gray-500" />
            ) : file.mime_type?.startsWith('image/') ? (
              <ImageIcon className="w-5 h-5 text-blue-500" />
            ) : file.mime_type?.startsWith('video/') ? (
              <Video className="w-5 h-5 text-red-500" />
            ) : file.mime_type?.startsWith('audio/') ? (
              <Music className="w-5 h-5 text-purple-500" />
            ) : (
              <FileText className="w-5 h-5 text-green-500" />
            )}
            <h3 className="font-medium text-gray-900 truncate max-w-md">{file.name}</h3>
          </div>
          <div className="flex items-center space-x-2">
            <button
              onClick={onDownload}
              className="p-2 rounded-lg hover:bg-gray-100 text-gray-600"
              title="下载"
            >
              <Download className="w-5 h-5" />
            </button>
            <button
              className="p-2 rounded-lg hover:bg-gray-100 text-gray-600"
              title="分享"
            >
              <Share2 className="w-5 h-5" />
            </button>
            <button
              onClick={onClose}
              className="p-2 rounded-lg hover:bg-gray-100 text-gray-600"
              title="关闭"
            >
              <X className="w-5 h-5" />
            </button>
          </div>
        </div>

        {/* 内容区域 */}
        <div className="flex-1 overflow-hidden">
          {loading && (
            <div className="flex items-center justify-center h-full text-gray-500">
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600"></div>
            </div>
          )}
          {!loading && getFilePreview()}
        </div>

        {/* 文件信息 */}
        <div className="px-4 py-3 border-t border-gray-200 bg-gray-50">
          <div className="flex items-center justify-between text-sm text-gray-600">
            <div>
              <span className="font-medium">{file.name}</span>
              <span className="ml-2">({formatFileSize(file.size)})</span>
            </div>
            <div>
              上传于 {new Date(file.created_at).toLocaleString('zh-CN')}
            </div>
          </div>
        </div>
      </div>
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
