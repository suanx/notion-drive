mod onedrive;

use aws_sdk_s3::{Client, Config, Region};
use sqlx::PgPool;
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;
use sha2::{Digest, Sha256};

use crate::config::{Config as AppConfig, MinioConfig};
use crate::storage::onedrive::{OneDriveConfig, OneDriveStorage};

/// 安全地清理文件名 - 防止路径遍历攻击
pub fn sanitize_filename(filename: &str) -> String {
    // 使用 Path::file_name 获取基本文件名（去除目录路径）
    let basic_name = Path::new(filename)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(filename);
    
    // 替换危险字符
    basic_name
        .replace(':', "-")
        .replace('?', "-")
        .replace('/', "_")
        .replace('\\', "_")
        .replace('\0', "_")
        .trim()
        .to_string()
}

#[derive(Debug)]
pub enum StorageError {
    IoError(std::io::Error),
    S3Error(aws_sdk_s3::Error),
    DatabaseError(sqlx::Error),
    InvalidPath(String),
    FileNotFound(String),
    QuotaExceeded(u64, u64), // (required, available)
    InvalidChunk(String),
    OnedriveError(String),
}

impl From<std::io::Error> for StorageError {
    fn from(err: std::io::Error) -> Self {
        StorageError::IoError(err)
    }
}

impl From<aws_sdk_s3::Error> for StorageError {
    fn from(err: aws_sdk_s3::Error) -> Self {
        StorageError::S3Error(err)
    }
}

impl From<sqlx::Error> for StorageError {
    fn from(err: sqlx::Error) -> Self {
        StorageError::DatabaseError(err)
    }
}

#[derive(Debug, Clone)]
pub struct StorageResult {
    pub hash: String,
    pub size: u64,
    pub storage_key: String,
    pub storage_policy_id: Uuid,
    pub is_duplicate: bool,
    pub storage_type: String, // "local", "s3", "onedrive"
}

#[derive(Debug, Clone)]
pub struct ChunkInfo {
    pub chunk_index: u32,
    pub chunk_size: u64,
    pub total_chunks: u32,
}

pub struct StorageManager {
    pub local_storage: PathBuf,
    pub s3_client: Option<Client>,
    pub default_policy_id: Uuid,
    pub db_pool: PgPool,
    pub onedrive_storages: Vec<(Uuid, OneDriveStorage)>, // (policy_id, storage)
}

impl StorageManager {
    pub async fn new(
        config: &AppConfig,
        minio_config: &MinioConfig,
        db_pool: PgPool,
    ) -> Result<Self, StorageError> {
        let local_storage = PathBuf::from(&config.storage.local_path);
        tokio::fs::create_dir_all(&local_storage).await?;

        let s3_client = if minio_config.endpoint != "localhost:9000" {
            let custom_config = Config::builder()
                .endpoint_url(format!("http://{}", minio_config.endpoint))
                .region(Region::new("us-east-1"))
                .credentials_provider(
                    aws_sdk_s3::config::Credentials::new(
                        &minio_config.access_key,
                        &minio_config.secret_key,
                        None,
                        None,
                        "custom",
                    ),
                )
                .build();

            Some(Client::new(&custom_config))
        } else {
            None
        };

        let default_policy_id = Uuid::parse_str(&config.storage.default_policy_id)
            .map_err(|_| StorageError::InvalidPath("Invalid default policy ID".to_string()))?;

        // 初始化 OneDrive 存储驱动
        let onedrive_storages = Self::initialize_onedrive_storages(&db_pool).await?;

        Ok(Self {
            local_storage,
            s3_client,
            default_policy_id,
            db_pool,
            onedrive_storages,
        })
    }

    /// 从数据库加载 OneDrive 存储配置
    async fn initialize_onedrive_storages(db_pool: &PgPool) -> Result<Vec<(Uuid, OneDriveStorage)>, StorageError> {
        let policies = sqlx::query!(
            r#"
            SELECT id, config FROM storage_policies 
            WHERE driver = 'onedrive' AND config IS NOT NULL
            "#
        )
        .fetch_all(db_pool)
        .await?;

        let mut storages = Vec::new();
        for policy in policies {
            let config: serde_json::Value = serde_json::from_str(&policy.config)
                .map_err(|e| StorageError::InvalidPath(format!("Invalid OneDrive config: {}", e)))?;
            
            let onedrive_config = OneDriveConfig {
                client_id: config["client_id"].as_str()
                    .ok_or(StorageError::InvalidPath("Missing client_id".to_string()))?
                    .to_string(),
                client_secret: config["client_secret"].as_str().map(String::from),
                tenant: config["tenant"].as_str()
                    .ok_or(StorageError::InvalidPath("Missing tenant".to_string()))?
                    .to_string(),
                drive_type: config["drive_type"].as_str()
                    .ok_or(StorageError::InvalidPath("Missing drive_type".to_string()))?
                    .to_string(),
                redirect_uri: config["redirect_uri"].as_str().map(String::from),
            };

            let storage = OneDriveStorage::new(onedrive_config, db_pool.clone());
            storages.push((policy.id, storage));
        }

        Ok(storages)
    }

    /// 获取 OneDrive 存储驱动
    fn get_onedrive_storage(&self, policy_id: Uuid) -> Option<&OneDriveStorage> {
        self.onedrive_storages
            .iter()
            .find(|(id, _)| *id == policy_id)
            .map(|(_, storage)| storage)
    }

    /// 检查文件是否已存在（秒传）
    pub async fn check_duplicate(&self, hash: &str, size: u64) -> Result<Option<StorageResult>, StorageError> {
        let existing = sqlx::query_as!(
            (Uuid, String, i64, String, String),
            r#"
                SELECT fb.id, fb.hash, fb.size, fb.storage_key, sp.driver as storage_type 
                FROM file_blobs fb
                JOIN storage_policies sp ON fb.storage_policy_id = sp.id
                WHERE fb.hash = $1 AND fb.size = $2
            "#,
            hash,
            size as i64
        )
        .fetch_optional(&self.db_pool)
        .await?;

        if let Some((blob_id, _, _, storage_key, storage_type)) = existing {
            // 增加引用计数
            sqlx::query!(
                "UPDATE file_blobs SET ref_count = ref_count + 1 WHERE id = $1",
                blob_id
            )
            .execute(&self.db_pool)
            .await?;

            Ok(Some(StorageResult {
                hash: hash.to_string(),
                size,
                storage_key,
                storage_policy_id: self.default_policy_id,
                is_duplicate: true,
                storage_type,
            }))
        } else {
            Ok(None)
        }
    }

    /// 检查用户配额
    pub async fn check_quota(&self, user_id: Uuid, required_size: u64) -> Result<(), StorageError> {
        let user = sqlx::query!(
            "SELECT quota_size, quota_used FROM users WHERE id = $1",
            user_id
        )
        .fetch_optional(&self.db_pool)
        .await?;

        let user = user.ok_or(StorageError::InvalidPath("User not found".to_string()))?;
        
        let available = user.quota_size - user.quota_used;
        if required_size > available as u64 {
            return Err(StorageError::QuotaExceeded(required_size, available as u64));
        }

        Ok(())
    }

    /// 更新用户配额
    pub async fn update_quota(&self, user_id: Uuid, delta: i64) -> Result<(), StorageError> {
        sqlx::query!(
            "UPDATE users SET quota_used = quota_used + $1 WHERE id = $2",
            delta,
            user_id
        )
        .execute(&self.db_pool)
        .await?;

        Ok(())
    }

    /// 上传文件到存储后端（支持 OneDrive）
    pub async fn upload_file(
        &self,
        user_id: Uuid,
        storage_policy_id: Uuid,
        content: &[u8],
    ) -> Result<StorageResult, StorageError> {
        // 计算文件哈希
        let mut hasher = Sha256::new();
        hasher.update(content);
        let hash = format!("{:x}", hasher.finalize());

        // 检查是否为重复文件（秒传）
        if let Some(result) = self.check_duplicate(&hash, content.len() as u64).await? {
            return Ok(result);
        }

        // 检查配额
        self.check_quota(user_id, content.len() as u64).await?;

        // 根据存储策略选择存储后端
        let storage_type = self.get_storage_type(&storage_policy_id).await?;

        let result = match storage_type.as_str() {
            "onedrive" => {
                self.upload_to_onedrive(user_id, storage_policy_id, content).await?
            }
            "s3" => {
                self.upload_to_s3(&content).await?
            }
            _ => {
                self.upload_to_local(&content).await?
            }
        };

        // 更新配额
        self.update_quota(user_id, content.len() as i64).await?;

        Ok(StorageResult {
            hash,
            size: content.len() as u64,
            storage_key: result.storage_key,
            storage_policy_id,
            is_duplicate: false,
            storage_type,
        })
    }

    /// 上传到 OneDrive
    async fn upload_to_onedrive(
        &self,
        user_id: Uuid,
        storage_policy_id: Uuid,
        content: &[u8],
    ) -> Result<StorageResult, StorageError> {
        let onedrive = self.get_onedrive_storage(storage_policy_id)
            .ok_or(StorageError::InvalidPath("OneDrive storage not found".to_string()))?;

        // 生成文件名
        let file_name = format!("notion-drive-{}", Uuid::new_v4());

        // 上传到 OneDrive
        let metadata = onedrive.upload_file(user_id, &file_name, content, None).await?;

        // 保存文件映射
        onedrive.save_file_mapping(
            Uuid::new_v4(), // 临时 file_id，实际会在创建文件记录时设置
            storage_policy_id,
            &metadata.id,
            &metadata.id, // drive_id 使用 file_id 简化处理
            &metadata.web_url,
            metadata.size,
        ).await?;

        Ok(StorageResult {
            hash: "".to_string(), // OneDrive 不提供哈希，需要单独计算
            size: metadata.size as u64,
            storage_key: metadata.id,
            storage_policy_id,
            is_duplicate: false,
            storage_type: "onedrive".to_string(),
        })
    }

    /// 上传到本地
    async fn upload_to_local(&self, content: &[u8]) -> Result<StorageResult, StorageError> {
        let storage_key = format!("files/{}", Uuid::new_v4());
        
        if let Some(parent) = self.local_storage.join(&storage_key).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        
        let mut file = File::create(self.local_storage.join(&storage_key)).await?;
        file.write_all(content).await?;
        file.flush().await?;

        Ok(StorageResult {
            hash: "".to_string(),
            size: content.len() as u64,
            storage_key,
            storage_policy_id: self.default_policy_id,
            is_duplicate: false,
            storage_type: "local".to_string(),
        })
    }

    /// 上传到 S3
    async fn upload_to_s3(&self, content: &[u8]) -> Result<StorageResult, StorageError> {
        let storage_key = format!("files/{}", Uuid::new_v4());

        if let Some(client) = &self.s3_client {
            client
                .put_object()
                .bucket("notion-drive")
                .key(&storage_key)
                .body(content.to_vec().into())
                .send()
                .await?;
        }

        Ok(StorageResult {
            hash: "".to_string(),
            size: content.len() as u64,
            storage_key,
            storage_policy_id: self.default_policy_id,
            is_duplicate: false,
            storage_type: "s3".to_string(),
        })
    }

    /// 获取存储类型
    async fn get_storage_type(&self, policy_id: &Uuid) -> Result<String, StorageError> {
        let policy = sqlx::query!(
            "SELECT driver FROM storage_policies WHERE id = $1",
            policy_id
        )
        .fetch_optional(&self.db_pool)
        .await?;

        let policy = policy.ok_or(StorageError::InvalidPath("Storage policy not found".to_string()))?;
        Ok(policy.driver)
    }

    /// 下载文件
    pub async fn download_file(
        &self,
        storage_key: &str,
        storage_type: &str,
        user_id: Option<Uuid>,
    ) -> Result<Vec<u8>, StorageError> {
        match storage_type {
            "onedrive" => {
                let user_id = user_id.ok_or(StorageError::InvalidPath("User ID required for OneDrive download".to_string()))?;
                let onedrive = self.get_onedrive_storage(self.default_policy_id)
                    .ok_or(StorageError::InvalidPath("OneDrive storage not found".to_string()))?;
                onedrive.download_file(user_id, storage_key).await
            }
            "s3" => {
                if let Some(client) = &self.s3_client {
                    let response = client
                        .get_object()
                        .bucket("notion-drive")
                        .key(storage_key)
                        .send()
                        .await?;

                    let body = response.body.collect().await?;
                    Ok(body.into_bytes().to_vec())
                } else {
                    Err(StorageError::InvalidPath("S3 client not available".to_string()))
                }
            }
            _ => {
                self.download_from_local(storage_key).await
            }
        }
    }

    /// 删除文件
    pub async fn delete_file(
        &self,
        storage_key: &str,
        storage_type: &str,
        user_id: Option<Uuid>,
    ) -> Result<(), StorageError> {
        match storage_type {
            "onedrive" => {
                let user_id = user_id.ok_or(StorageError::InvalidPath("User ID required for OneDrive delete".to_string()))?;
                let onedrive = self.get_onedrive_storage(self.default_policy_id)
                    .ok_or(StorageError::InvalidPath("OneDrive storage not found".to_string()))?;
                onedrive.delete_file(user_id, storage_key).await
            }
            "s3" => {
                if let Some(client) = &self.s3_client {
                    client
                        .delete_object()
                        .bucket("notion-drive")
                        .key(storage_key)
                        .send()
                        .await?;
                }
                Ok(())
            }
            _ => {
                self.delete_from_local(storage_key).await
            }
        }
    }

    /// 生成预签名 URL
    pub async fn generate_presigned_url(
        &self,
        storage_key: &str,
        storage_type: &str,
        expiration_seconds: u64,
    ) -> Result<String, StorageError> {
        match storage_type {
            "s3" => {
                if let Some(client) = &self.s3_client {
                    let request = client
                        .get_object()
                        .bucket("notion-drive")
                        .key(storage_key)
                        .presigned(expiration_seconds)
                        .await
                        .map_err(StorageError::S3Error)?;

                    Ok(request.uri().to_string())
                } else {
                    Err(StorageError::InvalidPath("S3 client not available".to_string()))
                }
            }
            "onedrive" => {
                // OneDrive 使用临时 URL，需要通过 Graph API 获取
                Err(StorageError::InvalidPath(
                    "Presigned URL for OneDrive requires additional implementation".to_string(),
                ))
            }
            _ => {
                Err(StorageError::InvalidPath(
                    "Presigned URL only available for S3 and OneDrive storage".to_string(),
                ))
            }
        }
    }

    // 本地存储实现
    async fn download_from_local(&self, storage_key: &str) -> Result<Vec<u8>, StorageError> {
        let file_path = self.local_storage.join(storage_key);
        let mut file = File::open(&file_path).await?;
        
        let mut contents = Vec::new();
        file.read_to_end(&mut contents).await?;
        
        Ok(contents)
    }

    async fn delete_from_local(&self, storage_key: &str) -> Result<(), StorageError> {
        let file_path = self.local_storage.join(storage_key);
        tokio::fs::remove_file(&file_path).await?;
        Ok(())
    }
}
