-- 第二阶段：核心功能增强
-- 1. 分块上传相关表
CREATE TABLE upload_chunks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    file_hash VARCHAR(64) NOT NULL,
    chunk_index INT NOT NULL,
    chunk_size BIGINT NOT NULL,
    storage_key VARCHAR(255),
    uploaded_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(file_hash, chunk_index)
);

CREATE INDEX idx_upload_chunks_user_id ON upload_chunks(user_id);
CREATE INDEX idx_upload_chunks_file_hash ON upload_chunks(file_hash);

-- 2. 上传会话表（用于 TUS 协议）
CREATE TABLE upload_sessions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    file_name VARCHAR(255) NOT NULL,
    file_size BIGINT NOT NULL,
    file_hash VARCHAR(64),
    chunk_size BIGINT DEFAULT 5242880, -- 默认 5MB
    uploaded_size BIGINT DEFAULT 0,
    status VARCHAR(20) DEFAULT 'active', -- 'active', 'completed', 'cancelled', 'expired'
    session_token VARCHAR(64) UNIQUE NOT NULL,
    metadata JSONB, -- 存储 MIME 类型等元数据
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX idx_upload_sessions_user_id ON upload_sessions(user_id);
CREATE INDEX idx_upload_sessions_token ON upload_sessions(session_token);
CREATE INDEX idx_upload_sessions_status ON upload_sessions(status);

-- 3. 文件版本控制表
CREATE TABLE file_versions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    file_id UUID NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    blob_id UUID NOT NULL REFERENCES file_blobs(id) ON DELETE CASCADE,
    version_number INT NOT NULL,
    size BIGINT NOT NULL,
    uploaded_by UUID NOT NULL REFERENCES users(id) ON DELETE SET NULL,
    upload_ip INET,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(file_id, version_number)
);

CREATE INDEX idx_file_versions_file_id ON file_versions(file_id);
CREATE INDEX idx_file_versions_blob_id ON file_versions(blob_id);

-- 4. 回收站增强（扩展软删除）
-- 修改 files 表，添加回收站相关字段
ALTER TABLE files ADD COLUMN IF NOT EXISTS trashed_at TIMESTAMP WITH TIME ZONE;
ALTER TABLE files ADD COLUMN IF NOT EXISTS trashed_by UUID REFERENCES users(id) ON DELETE SET NULL;

-- 5. 团队协作表
CREATE TABLE team_members (
    team_id UUID NOT NULL REFERENCES user_groups(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(20) NOT NULL DEFAULT 'member', -- 'owner', 'admin', 'member', 'viewer'
    invited_by UUID REFERENCES users(id) ON DELETE SET NULL,
    status VARCHAR(20) DEFAULT 'pending', -- 'pending', 'accepted', 'rejected'
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (team_id, user_id)
);

CREATE INDEX idx_team_members_user_id ON team_members(user_id);
CREATE INDEX idx_team_members_status ON team_members(status);

-- 6. 文件权限表（细粒度权限控制）
CREATE TABLE file_permissions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    file_id UUID NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    group_id UUID REFERENCES user_groups(id) ON DELETE CASCADE,
    can_read BOOLEAN DEFAULT false,
    can_write BOOLEAN DEFAULT false,
    can_delete BOOLEAN DEFAULT false,
    can_share BOOLEAN DEFAULT false,
    granted_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(file_id, user_id)
);

CREATE INDEX idx_file_permissions_file_id ON file_permissions(file_id);
CREATE INDEX idx_file_permissions_user_id ON file_permissions(user_id);
CREATE INDEX idx_file_permissions_group_id ON file_permissions(group_id);

-- 7. 下载历史记录（增强审计）
ALTER TABLE download_records ADD COLUMN IF NOT EXISTS user_agent TEXT;
ALTER TABLE download_records ADD COLUMN IF NOT EXISTS file_version_id UUID REFERENCES file_versions(id) ON DELETE SET NULL;

-- 8. 离线下载任务增强
ALTER TABLE offline_download_tasks ADD COLUMN IF NOT EXISTS downloaded_file_id UUID REFERENCES files(id) ON DELETE SET NULL;
ALTER TABLE offline_download_tasks ADD COLUMN IF NOT EXISTS retry_count INT DEFAULT 0;

-- 9. 更新 storage_policies 表，添加更多配置选项
ALTER TABLE storage_policies ADD COLUMN IF NOT EXISTS max_file_size BIGINT;
ALTER TABLE storage_policies ADD COLUMN IF NOT EXISTS allowed_mime_types TEXT[];
ALTER TABLE storage_policies ADD COLUMN IF NOT EXISTS virus_scan_enabled BOOLEAN DEFAULT false;

-- 10. 插入默认配置
INSERT INTO storage_policies (name, driver, config, is_default, max_file_size) VALUES
    ('本地存储 - 大文件', 'local', '{"path": "/app/storage", "chunk_size": 5242880}'::jsonb, false, 10737418240); -- 10GB

-- 11. 创建更新 updated_at 的触发器（新表）
CREATE TRIGGER update_upload_sessions_updated_at BEFORE UPDATE ON upload_sessions
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- 12. 视图：用户存储使用情况
CREATE OR REPLACE VIEW user_storage_usage AS
SELECT 
    u.id as user_id,
    u.username,
    u.email,
    u.quota_size,
    u.quota_used,
    COALESCE(SUM(CASE WHEN f.type = 'file' AND f.is_deleted = false THEN f.size ELSE 0 END), 0) as actual_used,
    COUNT(CASE WHEN f.type = 'file' AND f.is_deleted = false THEN 1 END) as file_count,
    COUNT(CASE WHEN f.type = 'folder' AND f.is_deleted = false THEN 1 END) as folder_count
FROM users u
LEFT JOIN files f ON u.id = f.user_id
GROUP BY u.id, u.username, u.email, u.quota_size, u.quota_used;

-- 13. 视图：文件版本历史
CREATE OR REPLACE VIEW file_version_history AS
SELECT 
    f.id as file_id,
    f.name,
    f.type,
    fv.version_number,
    fv.size as version_size,
    fv.uploaded_by,
    u.username as uploaded_by_username,
    fv.created_at as version_created_at
FROM files f
JOIN file_versions fv ON f.id = fv.file_id
LEFT JOIN users u ON fv.uploaded_by = u.id
ORDER BY f.id, fv.version_number DESC;
