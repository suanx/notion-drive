-- 初始化数据库脚本
-- 创建扩展
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pg_trgm"; -- 用于模糊搜索

-- 用户表
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    username VARCHAR(50) UNIQUE NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    avatar_url VARCHAR(255),
    quota_size BIGINT DEFAULT 10737418240, -- 默认 10GB
    quota_used BIGINT DEFAULT 0,
    is_active BOOLEAN DEFAULT true,
    is_admin BOOLEAN DEFAULT false,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- 用户组表
CREATE TABLE user_groups (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(100) NOT NULL,
    description TEXT,
    quota_size BIGINT DEFAULT 10737418240, -- 默认 10GB
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- 用户组关联表
CREATE TABLE user_group_members (
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    group_id UUID REFERENCES user_groups(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, group_id)
);

-- 存储策略表
CREATE TABLE storage_policies (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(100) NOT NULL,
    driver VARCHAR(50) NOT NULL, -- 'local', 's3', 'oss', 'sftp'
    config JSONB NOT NULL, -- 存储驱动配置
    is_default BOOLEAN DEFAULT false,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- 文件元数据表（逻辑文件）
CREATE TABLE files (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    parent_id UUID REFERENCES files(id) ON DELETE CASCADE,
    blob_id UUID, -- 指向 file_blobs.id，多个文件可能共享同一个 blob（硬链接）
    name VARCHAR(255) NOT NULL,
    type VARCHAR(20) NOT NULL, -- 'file', 'folder'
    mime_type VARCHAR(100),
    size BIGINT DEFAULT 0,
    storage_policy_id UUID REFERENCES storage_policies(id),
    path LTREE, -- 路径树，用于快速查询子目录
    is_deleted BOOLEAN DEFAULT false,
    deleted_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- 文件物理存储表（文件内容）
CREATE TABLE file_blobs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    hash VARCHAR(64) UNIQUE NOT NULL, -- SHA256
    size BIGINT NOT NULL,
    storage_key VARCHAR(255), -- 在存储后端中的键/路径
    storage_policy_id UUID NOT NULL REFERENCES storage_policies(id),
    upload_ip INET,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    ref_count INT DEFAULT 1 -- 引用计数，用于去重
);

-- 分享链接表
CREATE TABLE share_links (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    file_id UUID NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token VARCHAR(64) UNIQUE NOT NULL,
    password VARCHAR(100),
    expires_at TIMESTAMP WITH TIME ZONE,
    max_downloads INT, -- NULL 表示无限制
    download_count INT DEFAULT 0,
    allow_preview BOOLEAN DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- 下载记录表（用于审计和限速）
CREATE TABLE download_records (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    file_id UUID NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    share_link_id UUID REFERENCES share_links(id) ON DELETE SET NULL,
    ip_address INET,
    downloaded_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- 离线下载任务表
CREATE TABLE offline_download_tasks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source_url TEXT NOT NULL,
    file_name VARCHAR(255),
    status VARCHAR(20) DEFAULT 'pending', -- 'pending', 'downloading', 'completed', 'failed'
    progress INT DEFAULT 0,
    error_message TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- 创建索引
CREATE INDEX idx_files_user_id ON files(user_id);
CREATE INDEX idx_files_parent_id ON files(parent_id);
CREATE INDEX idx_files_blob_id ON files(blob_id);
CREATE INDEX idx_files_path ON files USING GIST(path);
CREATE INDEX idx_files_type ON files(type);
CREATE INDEX idx_files_is_deleted ON files(is_deleted) WHERE is_deleted = false;

CREATE INDEX idx_file_blobs_hash ON file_blobs(hash);
CREATE INDEX idx_file_blobs_storage_policy ON file_blobs(storage_policy_id);

CREATE INDEX idx_share_links_token ON share_links(token);
CREATE INDEX idx_share_links_file_id ON share_links(file_id);
CREATE INDEX idx_share_links_user_id ON share_links(user_id);

CREATE INDEX idx_download_records_file_id ON download_records(file_id);
CREATE INDEX idx_download_records_user_id ON download_records(user_id);
CREATE INDEX idx_download_records_created ON download_records(downloaded_at);

CREATE INDEX idx_offline_download_tasks_user_id ON offline_download_tasks(user_id);
CREATE INDEX idx_offline_download_tasks_status ON offline_download_tasks(status);

-- 创建更新 updated_at 的触发器函数
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ language 'plpgsql';

-- 为需要 updated_at 的表创建触发器
CREATE TRIGGER update_users_updated_at BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_user_groups_updated_at BEFORE UPDATE ON user_groups
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_storage_policies_updated_at BEFORE UPDATE ON storage_policies
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_files_updated_at BEFORE UPDATE ON files
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_share_links_updated_at BEFORE UPDATE ON share_links
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_offline_download_tasks_updated_at BEFORE UPDATE ON offline_download_tasks
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- 插入默认存储策略
INSERT INTO storage_policies (name, driver, config, is_default) VALUES
    ('本地存储', 'local', '{"path": "/app/storage"}'::jsonb, true),
    ('MinIO 存储', 's3', '{"endpoint": "minio:9000", "bucket": "notion-drive", "use_ssl": false}'::jsonb, false);

-- 插入默认管理员用户（密码: admin123，需要生产环境修改）
INSERT INTO users (username, email, password_hash, is_admin, quota_size) VALUES
    ('admin', 'admin@notion-drive.com', '$2b$12$LQv3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/X4.qL.5vK0N6Z0Z0S', true, 107374182400); -- 100GB
