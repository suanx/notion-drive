'use client';

import axios from 'axios';
import { useState, useEffect } from 'react';
import { User, AuthResponse, FileInfo, FileListResponse, ShareLink, ShareInfo, QuotaInfo, OfflineDownloadTask, RecycleBinItem, UploadSession, FileVersion, TeamInfo, TeamMember, WebDavToken, WebDavUsage } from '@/types';

// 使用相对路径（静态导出需要）
const API_URL = '/api/v1';

export const apiClient = axios.create({
  baseURL: API_URL,
  headers: {
    'Content-Type': 'application/json',
  },
});

// 请求拦截器 - 添加认证 token
apiClient.interceptors.request.use(
  (config) => {
    const token = localStorage.getItem('token');
    if (token) {
      config.headers.Authorization = `Bearer ${token}`;
    }
    return config;
  },
  (error) => {
    return Promise.reject(error);
  }
);

// 响应拦截器 - 处理错误
apiClient.interceptors.response.use(
  (response) => response,
  (error) => {
    if (error.response?.status === 401) {
      // Token 过期，清除本地存储
      localStorage.removeItem('token');
      localStorage.removeItem('user');
      window.location.href = '/login/';
    }
    return Promise.reject(error);
  }
);

// 认证 API
export const authApi = {
  register: (data: { username: string; email: string; password: string }) =>
    apiClient.post<AuthResponse>('/auth/register', data),
  
  login: (data: { email: string; password: string }) =>
    apiClient.post<AuthResponse>('/auth/login', data),
  
  refreshToken: (token: string) =>
    apiClient.post<AuthResponse>('/auth/refresh', { token }),
  
  getCurrentUser: () =>
    apiClient.get<User>('/auth/me'),
};

// 文件 API
export const fileApi = {
  list: (params: { parent_id?: string; page?: number; page_size?: number }) =>
    apiClient.get<FileListResponse>('/files', { params }),
  
  createFolder: (data: { name: string; parent_id?: string }) =>
    apiClient.post<FileInfo>('/files', data),
  
  uploadSimple: (file: File) => {
    const formData = new FormData();
    formData.append('file', file);
    return apiClient.post('/files/upload/simple', formData, {
      headers: { 'Content-Type': 'multipart/form-data' },
    });
  },
  
  createUploadSession: (data: { file_name: string; file_size: number; chunk_size?: number; mime_type?: string }) =>
    apiClient.post<UploadSession>('/files/upload/session', data),
  
  uploadChunk: (sessionId: string, chunkIndex: number, chunk: Blob) => {
    const formData = new FormData();
    formData.append('chunk', chunk);
    return apiClient.put(`/files/upload/chunk?session_id=${sessionId}&chunk_index=${chunkIndex}`, formData, {
      headers: { 'Content-Type': 'multipart/form-data' },
    });
  },
  
  completeUploadSession: (sessionId: string, fileHash: string) =>
    apiClient.put<FileInfo>(`/files/upload/session/${sessionId}/complete`, { file_hash: fileHash }),
  
  cancelUploadSession: (sessionId: string) =>
    apiClient.delete(`/files/upload/session/${sessionId}/cancel`),
  
  get: (fileId: string) =>
    apiClient.get<FileInfo>(`/files/${fileId}`),
  
  delete: (fileId: string) =>
    apiClient.delete(`/files/${fileId}`),
  
  move: (fileId: string, data: { parent_id: string }) =>
    apiClient.put<FileInfo>(`/files/${fileId}/move`, data),
  
  rename: (fileId: string, data: { name: string }) =>
    apiClient.put<FileInfo>(`/files/${fileId}/rename`, data),
  
  download: (fileId: string) =>
    apiClient.get(`/files/${fileId}/download`, {
      responseType: 'blob',
    }),
  
  getVersions: (fileId: string) =>
    apiClient.get<FileVersion[]>(`/files/${fileId}/versions`),
  
  search: (params: { q: string; page?: number; page_size?: number }) =>
    apiClient.get<FileListResponse>('/files/search', { params }),
};

// 分享 API
export const shareApi = {
  list: (params?: { page?: number; page_size?: number }) =>
    apiClient.get<ShareLink[]>('/shares', { params }),
  
  create: (data: { file_id: string; password?: string; expires_at?: string; max_downloads?: number; allow_preview?: boolean }) =>
    apiClient.post<ShareLink>('/shares', data),
  
  delete: (shareId: string) =>
    apiClient.delete(`/shares/${shareId}`),
  
  getPublic: (token: string, password?: string) =>
    apiClient.post<ShareInfo>(`/shares/public/${token}`, password ? { password } : undefined),
  
  downloadPublic: (token: string, password?: string) =>
    apiClient.get(`/shares/public/${token}/download`, {
      responseType: 'blob',
      data: password ? { password } : undefined,
    }),
};

// 用户 API
export const userApi = {
  getProfile: () =>
    apiClient.get<User>('/users/profile'),
  
  updateProfile: (data: { username?: string; avatar_url?: string }) =>
    apiClient.put<User>('/users/profile', data),
  
  getQuota: () =>
    apiClient.get<QuotaInfo>('/users/quota'),
  
  getRecycleBin: () =>
    apiClient.get<RecycleBinItem[]>('/users/recycle-bin'),
  
  restoreFromRecycleBin: (fileId: string) =>
    apiClient.put(`/users/recycle-bin/${fileId}/restore`),
  
  permanentDelete: (fileId: string) =>
    apiClient.delete(`/users/recycle-bin/${fileId}/delete`),
  
  listTeams: () =>
    apiClient.get<TeamInfo[]>('/teams'),
  
  createTeam: (data: { name: string; description?: string; quota_size?: number }) =>
    apiClient.post<TeamInfo>('/teams', data),
  
  deleteTeam: (teamId: string) =>
    apiClient.delete(`/teams/${teamId}`),
  
  listTeamMembers: (teamId: string) =>
    apiClient.get<TeamMember[]>(`/teams/${teamId}/members`),
  
  inviteTeamMember: (teamId: string, data: { email: string; role?: string }) =>
    apiClient.post(`/teams/${teamId}/members`, data),
  
  removeTeamMember: (teamId: string, userId: string) =>
    apiClient.delete(`/teams/${teamId}/members/${userId}`),
  
  setFilePermission: (teamId: string, fileId: string, data: { user_id?: string; group_id?: string; can_read: boolean; can_write: boolean; can_delete: boolean; can_share: boolean }) =>
    apiClient.put(`/teams/${teamId}/files/${fileId}/permissions`, data),
  
  // WebDAV API
  listWebDavTokens: () =>
    apiClient.get<WebDavToken[]>('/users/webdav-tokens'),
  
  createWebDavToken: (data: { description?: string; expires_in_hours?: number }) =>
    apiClient.post<WebDavToken>('/users/webdav-tokens', data),
  
  deleteWebDavToken: (tokenId: string) =>
    apiClient.delete(`/users/webdav-tokens/${tokenId}`),
  
  getWebDavUsage: () =>
    apiClient.get<WebDavUsage>('/users/webdav-usage'),
};

// 离线下载 API
export const offlineDownloadApi = {
  create: (data: { source_url: string; file_name?: string }) =>
    apiClient.post<OfflineDownloadTask>('/offline-download', data),
  
  list: () =>
    apiClient.get<OfflineDownloadTask[]>('/offline-download'),
  
  cancel: (taskId: string) =>
    apiClient.delete(`/offline-download/${taskId}`),
};

// 认证状态管理
export function useAuth() {
  const [user, setUser] = useState<User | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const token = localStorage.getItem('token');
    if (token) {
      authApi.getCurrentUser()
        .then((response) => {
          setUser(response.data);
        })
        .catch(() => {
          localStorage.removeItem('token');
          localStorage.removeItem('user');
        })
        .finally(() => setLoading(false));
    } else {
      setLoading(false);
    }
  }, []);

  const login = (token: string, userData: User) => {
    localStorage.setItem('token', token);
    localStorage.setItem('user', JSON.stringify(userData));
    setUser(userData);
  };

  const logout = () => {
    localStorage.removeItem('token');
    localStorage.removeItem('user');
    setUser(null);
  };

  return { user, loading, login, logout, isAuthenticated: !!user };
}
