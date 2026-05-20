// 类型定义

export interface User {
  id: string;
  username: string;
  email: string;
  avatar_url?: string;
  is_admin: boolean;
  quota_size?: number;
  quota_used?: number;
}

export interface AuthResponse {
  token: string;
  user: User;
  expires_at: number;
}

export interface FileInfo {
  id: string;
  name: string;
  type: 'file' | 'folder';
  size: number;
  mime_type?: string;
  parent_id?: string;
  created_at: string;
  updated_at: string;
  is_deleted: boolean;
  trashed_at?: string;
}

export interface FileListResponse {
  files: FileInfo[];
  total: number;
  page: number;
  page_size: number;
}

export interface ShareLink {
  id: string;
  file_id: string;
  file?: FileInfo;
  token: string;
  password?: string;
  expires_at?: string;
  max_downloads?: number;
  download_count: number;
  allow_preview: boolean;
  share_url: string;
  created_at: string;
}

export interface ShareInfo {
  id: string;
  file: FileInfo;
  allow_preview: boolean;
  requires_password: boolean;
  expires_at?: string;
  remaining_downloads?: number;
}

export interface QuotaInfo {
  total: number;
  used: number;
  available: number;
  usage_percentage: number;
  file_count: number;
  folder_count: number;
}

export interface OfflineDownloadTask {
  id: string;
  user_id: string;
  source_url: string;
  file_name?: string;
  status: 'pending' | 'downloading' | 'completed' | 'failed' | 'cancelled';
  progress: number;
  error_message?: string;
  created_at: string;
  updated_at: string;
}

export interface RecycleBinItem {
  id: string;
  name: string;
  type: 'file' | 'folder';
  size: number;
  trashed_at: string;
  trashed_by?: string;
  parent_id?: string;
}

export interface UploadSession {
  id: string;
  file_name: string;
  file_size: number;
  chunk_size: number;
  uploaded_size: number;
  status: 'active' | 'completed' | 'cancelled' | 'expired';
  session_token: string;
  expires_at: string;
}

export interface FileVersion {
  id: string;
  file_id: string;
  version_number: number;
  size: number;
  uploaded_by?: string;
  uploaded_by_username?: string;
  created_at: string;
}

export interface TeamInfo {
  id: string;
  name: string;
  description?: string;
  quota_size: number;
  created_at: string;
  role: string;
}

export interface TeamMember {
  user_id: string;
  username: string;
  email: string;
  role: string;
  status: string;
  invited_by?: string;
  invited_by_username?: string;
  created_at: string;
}

// WebDAV 相关类型
export interface WebDavToken {
  id: string;
  token: string;
  description?: string;
  is_active: boolean;
  last_used_at?: string;
  expires_at?: string;
  created_at: string;
}

export interface WebDavUsage {
  total_requests: number;
  upload_count: number;
  download_count: number;
  total_upload_bytes: number;
  total_download_bytes: number;
  last_access_at?: string;
}

// 请求类型
export interface RegisterRequest {
  username: string;
  email: string;
  password: string;
}

export interface LoginRequest {
  email: string;
  password: string;
}

export interface CreateFolderRequest {
  name: string;
  parent_id?: string;
}

export interface MoveFileRequest {
  parent_id: string;
}

export interface RenameFileRequest {
  name: string;
}

export interface CreateShareRequest {
  file_id: string;
  password?: string;
  expires_at?: string;
  max_downloads?: number;
  allow_preview?: boolean;
}

export interface UpdateProfileRequest {
  username?: string;
  avatar_url?: string;
}

export interface CreateUploadSessionRequest {
  file_name: string;
  file_size: number;
  chunk_size?: number;
  mime_type?: string;
}

export interface CompleteUploadRequest {
  file_hash: string;
}

export interface CreateTeamRequest {
  name: string;
  description?: string;
  quota_size?: number;
}

export interface InviteMemberRequest {
  email: string;
  role?: string;
}

export interface SetPermissionRequest {
  user_id?: string;
  group_id?: string;
  can_read: boolean;
  can_write: boolean;
  can_delete: boolean;
  can_share: boolean;
}

export interface CreateWebDavTokenRequest {
  description?: string;
  expires_in_hours?: number;
}
