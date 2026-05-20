-- 添加 OneDrive 存储策略支持
-- Phase 2.5: OneDrive 集成

-- 1. 更新 storage_policies 表，添加 OneDrive 支持
ALTER TABLE storage_policies ADD COLUMN IF NOT EXISTS driver_version VARCHAR(20) DEFAULT 'v1.0';

-- 2. 添加 OneDrive 认证令牌表
CREATE TABLE onedrive_tokens (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    storage_policy_id UUID REFERENCES storage_policies(id) ON DELETE SET NULL,
    access_token TEXT NOT NULL,
    refresh_token TEXT NOT NULL,
    token_type VARCHAR(50) DEFAULT 'Bearer',
    expires_in BIGINT, -- 秒
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    scope TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_onedrive_tokens_user_id ON onedrive_tokens(user_id);
CREATE INDEX idx_onedrive_tokens_policy_id ON onedrive_tokens(storage_policy_id);

-- 3. 添加 OneDrive 文件映射表（存储 OneDrive 中的文件 ID）
CREATE TABLE onedrive_file_mappings (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    file_id UUID NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    storage_policy_id UUID NOT NULL REFERENCES storage_policies(id) ON DELETE CASCADE,
    onedrive_file_id VARCHAR(255) NOT NULL, -- OneDrive 中的文件 ID
    onedrive_drive_id VARCHAR(255), -- OneDrive drive ID
    onedrive_path VARCHAR(2048), -- OneDrive 中的完整路径
    size BIGINT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(storage_policy_id, onedrive_file_id)
);

CREATE INDEX idx_onedrive_file_mappings_file_id ON onedrive_file_mappings(file_id);
CREATE INDEX idx_onedrive_file_mappings_policy_id ON onedrive_file_mappings(storage_policy_id);

-- 4. 添加 OAuth2 授权状态表
CREATE TABLE oauth_authorization_states (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider VARCHAR(50) NOT NULL, -- 'onedrive', 'google', etc.
    state VARCHAR(255) NOT NULL, -- CSRF 保护状态
    code VARCHAR(255), -- 授权码
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(user_id, provider)
);

-- 5. 更新 storage_policies 默认数据，添加 OneDrive 示例
INSERT INTO storage_policies (name, driver, config, is_default, max_file_size, allowed_mime_types) VALUES
    ('OneDrive 个人存储', 'onedrive', '{"client_id": "YOUR_CLIENT_ID", "tenant": "common", "drive_type": "personal"}'::jsonb, false, 107374182400, NULL),
    ('OneDrive 商业存储', 'onedrive', '{"client_id": "YOUR_CLIENT_ID", "tenant": "YOUR_TENANT_ID", "drive_type": "business"}'::jsonb, false, 107374182400, NULL)
ON CONFLICT (name) DO NOTHING;

-- 6. 创建更新 updated_at 的触发器（新表）
CREATE TRIGGER update_onedrive_tokens_updated_at BEFORE UPDATE ON onedrive_tokens
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_onedrive_file_mappings_updated_at BEFORE UPDATE ON onedrive_file_mappings
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- 7. 视图：用户存储使用情况（包含 OneDrive）
CREATE OR REPLACE VIEW user_storage_usage_extended AS
SELECT 
    u.id as user_id,
    u.username,
    u.email,
    u.quota_size,
    u.quota_used,
    COALESCE(SUM(CASE WHEN f.type = 'file' AND f.is_deleted = false THEN f.size ELSE 0 END), 0) as local_used,
    COUNT(CASE WHEN f.type = 'file' AND f.is_deleted = false THEN 1 END) as local_file_count,
    COUNT(CASE WHEN f.type = 'folder' AND f.is_deleted = false THEN 1 END) as local_folder_count,
    COUNT(om.id) as onedrive_file_count,
    COALESCE(SUM(om.size), 0) as onedrive_used
FROM users u
LEFT JOIN files f ON u.id = f.user_id
LEFT JOIN onedrive_file_mappings om ON u.id = (SELECT user_id FROM files WHERE id = om.file_id)
GROUP BY u.id, u.username, u.email, u.quota_size, u.quota_used;

-- 8. 存储策略驱动枚举视图
CREATE OR REPLACE VIEW storage_drivers AS
SELECT 
    'local' as driver,
    '本地存储' as display_name,
    '{"path": "string"}' as config_schema
UNION ALL
SELECT 
    's3' as driver,
    'S3 兼容存储' as display_name,
    '{"endpoint": "string", "bucket": "string", "access_key": "string", "secret_key": "string"}' as config_schema
UNION ALL
SELECT 
    'onedrive' as driver,
    'Microsoft OneDrive' as display_name,
    '{"client_id": "string", "tenant": "string", "drive_type": "personal|business"}' as config_schema
UNION ALL
SELECT 
    'google_drive' as driver,
    'Google Drive' as display_name,
    '{"client_id": "string", "client_secret": "string"}' as config_schema;
