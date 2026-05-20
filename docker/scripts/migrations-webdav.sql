-- WebDAV 集成支持
-- Phase 2.6: WebDAV 协议支持

-- 1. WebDAV 访问令牌表（用于 WebDAV 客户端认证）
CREATE TABLE webdav_tokens (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token VARCHAR(64) UNIQUE NOT NULL,
    description VARCHAR(255),
    is_active BOOLEAN DEFAULT true,
    last_used_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX idx_webdav_tokens_user_id ON webdav_tokens(user_id);
CREATE INDEX idx_webdav_tokens_token ON webdav_tokens(token);

-- 2. WebDAV 访问日志表
CREATE TABLE webdav_access_logs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_id UUID REFERENCES webdav_tokens(id) ON DELETE SET NULL,
    method VARCHAR(10) NOT NULL, -- PROPFIND, GET, PUT, DELETE, MKCOL, COPY, MOVE, LOCK, UNLOCK
    path VARCHAR(2048) NOT NULL,
    status_code INTEGER NOT NULL,
    response_size BIGINT,
    ip_address INET,
    user_agent TEXT,
    duration_ms INTEGER,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_webdav_access_logs_user_id ON webdav_access_logs(user_id);
CREATE INDEX idx_webdav_access_logs_created ON webdav_access_logs(created_at);
CREATE INDEX idx_webdav_access_logs_method ON webdav_access_logs(method);

-- 3. WebDAV 锁表（支持 WebDAV 锁机制）
CREATE TABLE webdav_locks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    resource_id UUID NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    lock_token VARCHAR(255) UNIQUE NOT NULL,
    lock_scope VARCHAR(20) DEFAULT 'exclusive', -- 'exclusive' or 'shared'
    lock_type VARCHAR(20) DEFAULT 'write', -- 'write' or 'read'
    owner VARCHAR(255),
    timeout INTEGER, -- 秒
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL
);

CREATE INDEX idx_webdav_locks_resource_id ON webdav_locks(resource_id);
CREATE INDEX idx_webdav_locks_user_id ON webdav_locks(user_id);
CREATE INDEX idx_webdav_locks_token ON webdav_locks(lock_token);

-- 4. 更新 storage_policies，添加 WebDAV 配置字段
ALTER TABLE storage_policies ADD COLUMN IF NOT EXISTS webdav_enabled BOOLEAN DEFAULT true;
ALTER TABLE storage_policies ADD COLUMN IF NOT EXISTS webdav_base_path VARCHAR(255) DEFAULT '/';

-- 5. 插入默认 WebDAV 配置
INSERT INTO storage_policies (name, driver, config, is_default, webdav_enabled, webdav_base_path) VALUES
    ('本地存储 - WebDAV 启用', 'local', '{"path": "/app/storage"}'::jsonb, false, true, '/')
ON CONFLICT (name) DO NOTHING;

-- 6. 创建更新触发器
CREATE TRIGGER update_webdav_tokens_updated_at BEFORE UPDATE ON webdav_tokens
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- 7. 视图：WebDAV 使用情况统计
CREATE OR REPLACE VIEW webdav_usage_stats AS
SELECT 
    u.id as user_id,
    u.username,
    COUNT(DISTINCT wal.id) as total_requests,
    COUNT(DISTINCT CASE WHEN wal.method = 'PUT' THEN wal.id END) as upload_count,
    COUNT(DISTINCT CASE WHEN wal.method = 'GET' THEN wal.id END) as download_count,
    SUM(CASE WHEN wal.method = 'PUT' THEN wal.response_size ELSE 0 END) as total_upload_bytes,
    SUM(CASE WHEN wal.method = 'GET' THEN wal.response_size ELSE 0 END) as total_download_bytes,
    MAX(wal.created_at) as last_access_at
FROM users u
LEFT JOIN webdav_access_logs wal ON u.id = wal.user_id
GROUP BY u.id, u.username;

-- 8. 视图：活跃 WebDAV 锁
CREATE OR REPLACE VIEW active_webdav_locks AS
SELECT 
    wl.id,
    wl.resource_id,
    f.name as resource_name,
    f.type as resource_type,
    wl.user_id,
    u.username as locked_by,
    wl.lock_token,
    wl.lock_scope,
    wl.lock_type,
    wl.owner,
    wl.expires_at,
    EXTRACT(EPOCH FROM (wl.expires_at - NOW())) as time_remaining_seconds
FROM webdav_locks wl
JOIN files f ON wl.resource_id = f.id
JOIN users u ON wl.user_id = u.id
WHERE wl.expires_at > NOW();

-- 9. 存储策略驱动枚举视图（更新）
CREATE OR REPLACE VIEW storage_drivers AS
SELECT 
    'local' as driver,
    '本地存储' as display_name,
    '{"path": "string", "webdav_enabled": "boolean"}' as config_schema
UNION ALL
SELECT 
    's3' as driver,
    'S3 兼容存储' as display_name,
    '{"endpoint": "string", "bucket": "string", "access_key": "string", "secret_key": "string", "webdav_enabled": "boolean"}' as config_schema
UNION ALL
SELECT 
    'onedrive' as driver,
    'Microsoft OneDrive' as display_name,
    '{"client_id": "string", "tenant": "string", "drive_type": "personal|business", "webdav_enabled": "boolean"}' as config_schema;
