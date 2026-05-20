use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::storage::StorageError;

/// OneDrive 存储驱动
pub struct OneDriveStorage {
    client: Client,
    config: OneDriveConfig,
    pool: PgPool,
    token_cache: Arc<RwLock<Option<OneDriveToken>>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OneDriveConfig {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub tenant: String,
    pub drive_type: String, // "personal" or "business"
    pub redirect_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OneDriveToken {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub expires_at: DateTime<Utc>,
    pub scope: String,
}

#[derive(Debug, Serialize)]
pub struct OneDriveUploadSession {
    pub upload_url: String,
    pub expiration_datetime: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct OneDriveFileMetadata {
    pub id: String,
    pub name: String,
    pub size: i64,
    pub web_url: String,
    pub created_datetime: DateTime<Utc>,
    pub last_modified_datetime: DateTime<Utc>,
}

impl OneDriveStorage {
    pub fn new(config: OneDriveConfig, pool: PgPool) -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .expect("Failed to create HTTP client"),
            config,
            pool,
            token_cache: Arc::new(RwLock::new(None)),
        }
    }

    /// 获取授权 URL（OAuth2 授权码流程）
    pub fn get_authorization_url(&self, user_id: Uuid, state: &str) -> Result<String, StorageError> {
        let tenant = &self.config.tenant;
        let client_id = &self.config.client_id;
        let redirect_uri = self.config.redirect_uri.as_deref().unwrap_or("http://localhost:8080/api/v1/storage/onedrive/callback");
        
        let scopes = "Files.Read Files.ReadWrite offline_access";
        
        let url = format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/authorize?client_id={}&response_type=code&redirect_uri={}&scope={}&state={}",
            tenant, client_id, urlencoding::encode(redirect_uri), scopes, state
        );

        Ok(url)
    }

    /// 处理授权回调，交换授权码为令牌
    pub async fn exchange_code_for_token(&self, user_id: Uuid, code: &str) -> Result<OneDriveToken, StorageError> {
        let redirect_uri = self.config.redirect_uri.as_deref().unwrap_or("http://localhost:8080/api/v1/storage/onedrive/callback");
        
        let token_url = format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            self.config.tenant
        );

        let mut form = std::collections::HashMap::new();
        form.insert("client_id", &self.config.client_id);
        form.insert("client_secret", self.config.client_secret.as_deref().unwrap_or(""));
        form.insert("code", code);
        form.insert("redirect_uri", redirect_uri);
        form.insert("grant_type", "authorization_code");

        let response = self.client
            .post(&token_url)
            .form(&form)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(StorageError::InvalidPath(format!("Token exchange failed: {}", error_text)));
        }

        let token: TokenResponse = response.json().await?;
        
        let one_drive_token = OneDriveToken {
            access_token: token.access_token,
            refresh_token: token.refresh_token,
            token_type: token.token_type,
            expires_in: token.expires_in,
            expires_at: Utc::now() + chrono::Duration::seconds(token.expires_in as i64),
            scope: token.scope,
        };

        // 保存到数据库
        self.save_token(user_id, &one_drive_token).await?;
        
        // 更新缓存
        {
            let mut cache = self.token_cache.write().await;
            *cache = Some(one_drive_token.clone());
        }

        Ok(one_drive_token)
    }

    /// 刷新访问令牌
    pub async fn refresh_access_token(&self, user_id: Uuid) -> Result<OneDriveToken, StorageError> {
        // 从数据库获取刷新令牌
        let token_data = sqlx::query!(
            "SELECT access_token, refresh_token, token_type, expires_in, expires_at, scope FROM onedrive_tokens WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1",
            user_id
        )
        .fetch_optional(&self.pool)
        .await?;

        let token_data = match token_data {
            Some(t) => t,
            None => return Err(StorageError::InvalidPath("No OneDrive token found".to_string())),
        };

        if token_data.expires_at > Utc::now() {
            // 令牌仍有效
            return Ok(OneDriveToken {
                access_token: token_data.access_token,
                refresh_token: token_data.refresh_token,
                token_type: token_data.token_type,
                expires_in: token_data.expires_in,
                expires_at: token_data.expires_at,
                scope: token_data.scope,
            });
        }

        let refresh_token = token_data.refresh_token;
        
        let token_url = format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            self.config.tenant
        );

        let mut form = std::collections::HashMap::new();
        form.insert("client_id", &self.config.client_id);
        form.insert("client_secret", self.config.client_secret.as_deref().unwrap_or(""));
        form.insert("refresh_token", &refresh_token);
        form.insert("grant_type", "refresh_token");

        let response = self.client
            .post(&token_url)
            .form(&form)
            .send()
            .await?;

        if !response.status().is_success() {
            // 刷新失败，可能需要重新授权
            return Err(StorageError::InvalidPath("Token refresh failed, re-authorization required".to_string()));
        }

        let token: TokenResponse = response.json().await?;
        
        let one_drive_token = OneDriveToken {
            access_token: token.access_token,
            refresh_token: token.refresh_token,
            token_type: token.token_type,
            expires_in: token.expires_in,
            expires_at: Utc::now() + chrono::Duration::seconds(token.expires_in as i64),
            scope: token.scope,
        };

        // 更新数据库
        self.save_token(user_id, &one_drive_token).await?;
        
        // 更新缓存
        {
            let mut cache = self.token_cache.write().await;
            *cache = Some(one_drive_token.clone());
        }

        Ok(one_drive_token)
    }

    /// 获取当前有效的访问令牌
    pub async fn get_access_token(&self, user_id: Uuid) -> Result<String, StorageError> {
        // 检查缓存
        {
            let cache = self.token_cache.read().await;
            if let Some(token) = cache.as_ref() {
                if token.expires_at > Utc::now() + chrono::Duration::minutes(5) {
                    return Ok(token.access_token.clone());
                }
            }
        }

        // 缓存无效或过期，刷新令牌
        let token = self.refresh_access_token(user_id).await?;
        Ok(token.access_token)
    }

    /// 获取 OneDrive drive ID
    pub async fn get_drive_id(&self, user_id: Uuid) -> Result<String, StorageError> {
        let access_token = self.get_access_token(user_id).await?;
        
        let drive_type = if self.config.drive_type == "business" {
            "business"
        } else {
            "me"
        };

        let url = format!(
            "https://graph.microsoft.com/v1.0/{}/drive",
            drive_type
        );

        let response = self.client
            .get(&url)
            .bearer_auth(&access_token)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(StorageError::InvalidPath("Failed to get drive information".to_string()));
        }

        let drive: DriveResponse = response.json().await?;
        Ok(drive.id)
    }

use crate::storage::sanitize_filename;

/// 安全地清理文件名 - 防止路径遍历攻击
fn sanitize_filename_local(filename: &str) -> String {
    sanitize_filename(filename)
}

    /// 上传文件到 OneDrive
    async fn upload_file(
        &self,
        user_id: Uuid,
        parent_path: Option<&str>,
        file_name: &str,
        content: &[u8],
    ) -> Result<OneDriveFileMetadata, StorageError> {
        let access_token = self.get_access_token(user_id).await?;
        let drive_id = self.get_drive_id(user_id).await?;

        // 构建上传 URL - 使用清理后的文件名
        let path = parent_path.map(|p| format!("/{}", p.trim_start_matches('/'))).unwrap_or_default();
        let safe_name = sanitize_filename_local(file_name);
        
        let url = format!(
            "https://graph.microsoft.com/v1.0/me/drive/root:{}:/{}:content",
            path, safe_name
        );

        // 检查文件大小，决定使用简单上传还是分块上传
        if content.len() <= 4 * 1024 * 1024 {
            // 小文件：简单上传
            self.simple_upload(&url, &access_token, content, file_name).await
        } else {
            // 大文件：创建上传会话
            self.chunked_upload(user_id, &drive_id, file_name, content, parent_path).await
        }
    }

    /// 简单上传（小文件）
    async fn simple_upload(
        &self,
        url: &str,
        access_token: &str,
        content: &[u8],
        file_name: &str,
    ) -> Result<OneDriveFileMetadata, StorageError> {
        let response = self.client
            .put(url)
            .bearer_auth(access_token)
            .header("Content-Type", "application/octet-stream")
            .header("Content-Length", content.len())
            .body(content.to_vec())
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(StorageError::InvalidPath(format!("Upload failed: {}", error_text)));
        }

        let metadata: OneDriveFileMetadata = response.json().await?;
        Ok(metadata)
    }

    /// 分块上传（大文件）
    async fn chunked_upload(
        &self,
        user_id: Uuid,
        drive_id: &str,
        file_name: &str,
        content: &[u8],
        parent_path: Option<&str>,
    ) -> Result<OneDriveFileMetadata, StorageError> {
        let access_token = self.get_access_token(user_id).await?;
        
        let path = parent_path.map(|p| format!("/{}", p.trim_start_matches('/'))).unwrap_or_default();
        let safe_name = file_name.replace(':', "-").replace('?', "-");

        // 创建上传会话
        let session_url = format!(
            "https://graph.microsoft.com/v1.0/me/drive/items/root:/{}:/{}:createUploadSession",
            path, safe_name
        );

        let create_session_response = self.client
            .post(&session_url)
            .bearer_auth(&access_token)
            .json(&serde_json::json!({
                "item": {
                    "@microsoft.graph.conflictBehavior": "rename"
                }
            }))
            .send()
            .await?;

        if !create_session_response.status().is_success() {
            let error_text = create_session_response.text().await?;
            return Err(StorageError::InvalidPath(format!("Create upload session failed: {}", error_text)));
        }

        let session: OneDriveUploadSession = create_session_response.json().await?;
        
        // 分块上传
        let chunk_size = 5 * 1024 * 1024; // 5MB
        let total_chunks = (content.len() as f64 / chunk_size as f64).ceil() as usize;
        
        for chunk_index in 0..total_chunks {
            let start = chunk_index * chunk_size;
            let end = std::cmp::min(start + chunk_size, content.len());
            let chunk = &content[start..end];
            
            let range_header = format!("bytes {}-{}/{}", start, end - 1, content.len());
            
            let response = self.client
                .put(&session.upload_url)
                .bearer_auth(&access_token)
                .header("Content-Range", range_header)
                .header("Content-Type", "application/octet-stream")
                .body(chunk.to_vec())
                .send()
                .await?;

            if !response.status().is_success() && response.status() != StatusCode::ACCEPTED {
                let error_text = response.text().await?;
                return Err(StorageError::InvalidPath(format!("Chunk upload failed: {}", error_text)));
            }
        }

        // 获取上传完成的文件元数据
        // 注意：最后一个 chunk 的响应会包含文件元数据
        // 简化处理：重新获取文件信息
        let file_metadata = self.get_file_metadata(user_id, file_name, parent_path).await?;
        
        Ok(file_metadata)
    }

    /// 下载文件
    pub async fn download_file(
        &self,
        user_id: Uuid,
        onedrive_file_id: &str,
    ) -> Result<Vec<u8>, StorageError> {
        let access_token = self.get_access_token(user_id).await?;
        
        let url = format!(
            "https://graph.microsoft.com/v1.0/me/drive/items/{}/content",
            onedrive_file_id
        );

        let response = self.client
            .get(&url)
            .bearer_auth(&access_token)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(StorageError::FileNotFound(format!(
                "OneDrive file not found: {}",
                onedrive_file_id
            )));
        }

        let bytes = response.bytes().await?;
        Ok(bytes.to_vec())
    }

    /// 删除文件
    pub async fn delete_file(
        &self,
        user_id: Uuid,
        onedrive_file_id: &str,
    ) -> Result<(), StorageError> {
        let access_token = self.get_access_token(user_id).await?;
        
        let url = format!(
            "https://graph.microsoft.com/v1.0/me/drive/items/{}",
            onedrive_file_id
        );

        let response = self.client
            .delete(&url)
            .bearer_auth(&access_token)
            .send()
            .await?;

        if !response.status().is_success() && response.status() != StatusCode::NO_CONTENT {
            let error_text = response.text().await?;
            return Err(StorageError::InvalidPath(format!("Delete failed: {}", error_text)));
        }

        Ok(())
    }

    /// 获取文件元数据
    pub async fn get_file_metadata(
        &self,
        user_id: Uuid,
        file_name: &str,
        parent_path: Option<&str>,
    ) -> Result<OneDriveFileMetadata, StorageError> {
        let access_token = self.get_access_token(user_id).await?;
        
        let path = parent_path.map(|p| format!("/{}", p.trim_start_matches('/'))).unwrap_or_default();
        let safe_name = file_name.replace(':', "-").replace('?', "-");
        
        let url = format!(
            "https://graph.microsoft.com/v1.0/me/drive/root:{}:/{}",
            path, safe_name
        );

        let response = self.client
            .get(&url)
            .bearer_auth(&access_token)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(StorageError::FileNotFound(format!(
                "OneDrive file not found: {}",
                file_name
            )));
        }

        let metadata: OneDriveFileMetadata = response.json().await?;
        Ok(metadata)
    }

    /// 保存令牌到数据库
    async fn save_token(&self, user_id: Uuid, token: &OneDriveToken) -> Result<(), StorageError> {
        sqlx::query!(
            r#"
            INSERT INTO onedrive_tokens (user_id, access_token, refresh_token, token_type, expires_in, expires_at, scope)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (user_id) DO UPDATE SET
                access_token = $2,
                refresh_token = $3,
                token_type = $4,
                expires_in = $5,
                expires_at = $6,
                scope = $7,
                updated_at = CURRENT_TIMESTAMP
            "#,
            user_id,
            token.access_token,
            token.refresh_token,
            token.token_type,
            token.expires_in,
            token.expires_at,
            token.scope
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 保存文件映射
    pub async fn save_file_mapping(
        &self,
        file_id: Uuid,
        storage_policy_id: Uuid,
        onedrive_file_id: &str,
        onedrive_drive_id: &str,
        onedrive_path: &str,
        size: i64,
    ) -> Result<(), StorageError> {
        sqlx::query!(
            r#"
            INSERT INTO onedrive_file_mappings 
                (file_id, storage_policy_id, onedrive_file_id, onedrive_drive_id, onedrive_path, size)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (storage_policy_id, onedrive_file_id) DO UPDATE SET
                file_id = $1,
                size = $6,
                updated_at = CURRENT_TIMESTAMP
            "#,
            file_id,
            storage_policy_id,
            onedrive_file_id,
            onedrive_drive_id,
            onedrive_path,
            size
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 获取文件映射
    pub async fn get_file_mapping(
        &self,
        storage_policy_id: Uuid,
        onedrive_file_id: &str,
    ) -> Result<Option<(Uuid, i64)>, StorageError> {
        let mapping = sqlx::query_as!(
            (Uuid, i64),
            "SELECT file_id, size FROM onedrive_file_mappings WHERE storage_policy_id = $1 AND onedrive_file_id = $2",
            storage_policy_id,
            onedrive_file_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(mapping)
    }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    token_type: String,
    expires_in: i64,
    scope: String,
}

#[derive(Debug, Deserialize)]
struct DriveResponse {
    id: String,
    drive_type: String,
}
