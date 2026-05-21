use axum::{
    extract::{Path, Query, State, Multipart},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    auth::AuthExtractor,
    storage::{StorageError, StorageManager, StorageResult},
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct ListFilesQuery {
    pub parent_id: Option<Uuid>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct CreateFolderRequest {
    pub name: String,
    pub parent_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct MoveFileRequest {
    pub parent_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct RenameFileRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateUploadSessionRequest {
    pub file_name: String,
    pub file_size: u64,
    pub chunk_size: Option<u64>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UploadChunkRequest {
    pub chunk_index: u32,
    pub total_chunks: u32,
}

#[derive(Debug, Deserialize)]
pub struct CompleteUploadRequest {
    pub file_hash: String,
}

#[derive(Debug, Serialize)]
pub struct FileInfo {
    pub id: Uuid,
    pub name: String,
    pub type_: String,
    pub size: u64,
    pub mime_type: Option<String>,
    pub parent_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub is_deleted: bool,
    pub trashed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize)]
pub struct FileListResponse {
    pub files: Vec<FileInfo>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Serialize)]
pub struct UploadSessionResponse {
    pub id: Uuid,
    pub file_name: String,
    pub file_size: u64,
    pub chunk_size: u64,
    pub uploaded_size: u64,
    pub status: String,
    pub session_token: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct FileVersion {
    pub id: Uuid,
    pub file_id: Uuid,
    pub version_number: i32,
    pub size: i64,
    pub uploaded_by: Option<Uuid>,
    pub uploaded_by_username: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_files(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Query(query): Query<ListFilesQuery>,
) -> Result<Json<FileListResponse>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    let parent_id = query.parent_id;
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20);
    let offset = (page - 1) * page_size;

    // 查询文件列表（不包括回收站）
    let (files, total) = sqlx::query_as::<(Uuid, String, String, i64, Option<String>, Option<Uuid>, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, bool, Option<chrono::DateTime<chrono::Utc>>, i64), _>(
        r#"
            SELECT 
                f.id, f.name, f.type, f.size, f.mime_type, f.parent_id,
                f.created_at, f.updated_at, f.is_deleted, f.trashed_at,
                COUNT(*) OVER() as total
            FROM files f
            WHERE f.user_id = $1 
                AND f.parent_id = $2 
                AND f.is_deleted = false
            ORDER BY f.type DESC, f.name ASC
            LIMIT $3 OFFSET $4
        "#,
        claims.sub,
        parent_id,
        page_size as i64,
        offset as i64
    )
    .fetch_all(&state.db_pool)
    .await?;

    let file_list: Vec<FileInfo> = files
        .into_iter()
        .map(|(id, name, type_, size, mime_type, parent_id, created_at, updated_at, is_deleted, trashed_at, _total)| {
            FileInfo {
                id,
                name,
                type_,
                size: size as u64,
                mime_type,
                parent_id,
                created_at,
                updated_at,
                is_deleted,
                trashed_at,
            }
        })
        .collect();

    let total = file_list.first().map(|_| file_list.len() as u64).unwrap_or(0);

    Ok(Json(FileListResponse {
        files: file_list,
        total,
        page,
        page_size,
    }))
}

pub async fn create_folder(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Json(payload): Json<CreateFolderRequest>,
) -> Result<Json<FileInfo>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    // 验证父目录是否存在且属于当前用户
    if let Some(parent_id) = payload.parent_id {
        let parent = sqlx::query!(
            "SELECT id FROM files WHERE id = $1 AND user_id = $2 AND type = 'folder' AND is_deleted = false",
            parent_id,
            claims.sub
        )
        .fetch_optional(&state.db_pool)
        .await?;

        if parent.is_none() {
            return Err(AppError::BadRequest("Parent folder not found".to_string()));
        }
    }

    // 创建文件夹
    let file_id = sqlx::query_scalar!(
        r#"
            INSERT INTO files (user_id, parent_id, name, type, size)
            VALUES ($1, $2, $3, 'folder', 0)
            RETURNING id
        "#,
        claims.sub,
        payload.parent_id,
        payload.name
    )
    .fetch_one(&state.db_pool)
    .await?;

    let file = sqlx::query_as!(
        FileInfo,
        "SELECT id, name, type as type_, size, mime_type, parent_id, created_at, updated_at, is_deleted, trashed_at FROM files WHERE id = $1",
        file_id
    )
    .fetch_one(&state.db_pool)
    .await?;

    Ok(Json(file))
}

/// 简单上传（小文件，支持秒传）
pub async fn upload_file_simple(
    State(state): State<AppState>,
    auth: AuthExtractor,
    mut request: Multipart,
) -> Result<Json<FileInfo>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    // 读取文件内容
    let mut content = Vec::new();
    let mut file_name = String::new();
    let mut mime_type = None;

    while let Some(field) = request.next_field().await? {
        if field.name() == Some("file") {
            content.extend_from_slice(&field.bytes().await?);
            file_name = field.file_name().unwrap_or("uploaded_file").to_string();
            mime_type = field.content_type().map(|s| s.to_string());
        }
    }

    if content.is_empty() {
        return Err(AppError::BadRequest("Empty file".to_string()));
    }

    // 上传到存储后端（支持秒传）
    let storage_result = state
        .storage_manager
        .upload_file(claims.sub, state.config.storage.default_policy_id.parse()?, &content)
        .await?;

    // 检查是否为重复文件
    if storage_result.is_duplicate {
        // 文件已存在，创建引用（硬链接）
        let created_file_id = create_file_record(
            &state.db_pool,
            claims.sub,
            &file_name,
            storage_result.hash,
            storage_result.size as i64,
            storage_result.storage_policy_id,
            mime_type,
        ).await?;

        let file = get_file_info_internal(&state.db_pool, created_file_id, claims.sub).await?;
        return Ok(Json(file));
    }

    // 创建文件记录
    let created_file_id = create_file_record(
        &state.db_pool,
        claims.sub,
        &file_name,
        storage_result.hash,
        storage_result.size as i64,
        storage_result.storage_policy_id,
        mime_type,
    ).await?;

    // 创建文件版本记录
    create_file_version(
        &state.db_pool,
        created_file_id,
        1,
        storage_result.size as i64,
        claims.sub,
    ).await?;

    let file = get_file_info_internal(&state.db_pool, created_file_id, claims.sub).await?;

    Ok(Json(file))
}

/// 创建上传会话（大文件分块上传）
pub async fn create_upload_session(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Json(payload): Json<CreateUploadSessionRequest>,
) -> Result<Json<UploadSessionResponse>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    let chunk_size = payload.chunk_size.unwrap_or(5 * 1024 * 1024); // 默认 5MB

    let session_id = state
        .storage_manager
        .create_upload_session(
            claims.sub,
            &payload.file_name,
            payload.file_size,
            chunk_size,
            payload.mime_type.map(|m| serde_json::json!({"mime_type": m})),
        )
        .await?;

    let session = sqlx::query_as!(
        UploadSessionResponse,
        "SELECT id, file_name, file_size, chunk_size, uploaded_size, status, session_token, expires_at FROM upload_sessions WHERE id = $1",
        session_id
    )
    .fetch_one(&state.db_pool)
    .await?;

    Ok(Json(session))
}

/// 上传文件块
pub async fn upload_chunk(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Path(session_id): Path<Uuid>,
    Query(_query): Query<UploadChunkRequest>,
    mut request: Multipart,
) -> Result<StatusCode, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    // 读取块数据
    let mut content = Vec::new();
    while let Some(field) = request.next_field().await? {
        if field.name() == Some("chunk") {
            content.extend_from_slice(&field.bytes().await?);
        }
    }

    if content.is_empty() {
        return Err(AppError::BadRequest("Empty chunk".to_string()));
    }

    // 上传块
    state
        .storage_manager
        .upload_chunk(session_id, _query.chunk_index, &content)
        .await?;

    Ok(StatusCode::OK)
}

/// 完成上传会话
pub async fn complete_upload_session(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Path(session_id): Path<Uuid>,
    Json(payload): Json<CompleteUploadRequest>,
) -> Result<Json<FileInfo>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    // 完成上传会话
    let storage_result = state
        .storage_manager
        .complete_upload_session(session_id, &payload.file_hash)
        .await?;

    // 获取会话信息以获取文件名
    let session = sqlx::query!(
        "SELECT file_name, metadata FROM upload_sessions WHERE id = $1",
        session_id
    )
    .fetch_one(&state.db_pool)
    .await?;

    let file_name = session.file_name;
    let mime_type = session
        .metadata
        .and_then(|m| m.get("mime_type").map(|v| v.as_str().map(String::from)).flatten());

    // 创建文件记录
    let created_file_id = create_file_record(
        &state.db_pool,
        claims.sub,
        &file_name,
        storage_result.hash,
        storage_result.size as i64,
        storage_result.storage_policy_id,
        mime_type,
    ).await?;

    // 创建文件版本记录
    create_file_version(
        &state.db_pool,
        created_file_id,
        1,
        storage_result.size as i64,
        claims.sub,
    ).await?;

    let file = get_file_info_internal(&state.db_pool, created_file_id, claims.sub).await?;

    Ok(Json(file))
}

/// 取消上传会话
pub async fn cancel_upload_session(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Path(session_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let _claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    state
        .storage_manager
        .cancel_upload_session(session_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_file_info(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Path(file_id): Path<Uuid>,
) -> Result<Json<FileInfo>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    let file = get_file_info_internal(&state.db_pool, file_id, claims.sub).await?;

    Ok(Json(file))
}

async fn get_file_info_internal(
    db_pool: &PgPool,
    file_id: Uuid,
    user_id: Uuid,
) -> Result<FileInfo, AppError> {
    let file = sqlx::query_as!(
        FileInfo,
        "SELECT id, name, type as type_, size, mime_type, parent_id, created_at, updated_at, is_deleted, trashed_at FROM files WHERE id = $1 AND user_id = $2",
        file_id,
        user_id
    )
    .fetch_optional(db_pool)
    .await?;

    match file {
        Some(file) => Ok(file),
        None => Err(AppError::NotFound("File not found".to_string())),
    }
}

pub async fn delete_file(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Path(file_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    // 软删除：移动到回收站
    sqlx::query!(
        r#"
            UPDATE files 
            SET is_deleted = true, trashed_at = $1, trashed_by = $2
            WHERE id = $3 AND user_id = $4 AND is_deleted = false
        "#,
        chrono::Utc::now(),
        claims.sub,
        file_id,
        claims.sub
    )
    .execute(&state.db_pool)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn move_file(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Path(file_id): Path<Uuid>,
    Json(payload): Json<MoveFileRequest>,
) -> Result<Json<FileInfo>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    // 验证目标文件夹
    let target_folder = sqlx::query!(
        "SELECT id FROM files WHERE id = $1 AND user_id = $2 AND type = 'folder' AND is_deleted = false",
        payload.parent_id,
        claims.sub
    )
    .fetch_optional(&state.db_pool)
    .await?;

    if target_folder.is_none() {
        return Err(AppError::BadRequest("Target folder not found".to_string()));
    }

    // 移动文件
    sqlx::query!(
        "UPDATE files SET parent_id = $1 WHERE id = $2 AND user_id = $3",
        payload.parent_id,
        file_id,
        claims.sub
    )
    .execute(&state.db_pool)
    .await?;

    let file = get_file_info_internal(&state.db_pool, file_id, claims.sub).await?;

    Ok(Json(file))
}

pub async fn rename_file(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Path(file_id): Path<Uuid>,
    Json(payload): Json<RenameFileRequest>,
) -> Result<Json<FileInfo>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    sqlx::query!(
        "UPDATE files SET name = $1 WHERE id = $2 AND user_id = $3",
        payload.name,
        file_id,
        claims.sub
    )
    .execute(&state.db_pool)
    .await?;

    let file = get_file_info_internal(&state.db_pool, file_id, claims.sub).await?;

    Ok(Json(file))
}

pub async fn download_file(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Path(file_id): Path<Uuid>,
) -> Result<axum::response::Response, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    // 获取文件信息
    let file = sqlx::query!(
        "SELECT fb.storage_key, fb.size, f.name, f.mime_type, sp.driver as storage_type FROM files f JOIN file_blobs fb ON f.blob_id = fb.id JOIN storage_policies sp ON fb.storage_policy_id = sp.id WHERE f.id = $1 AND f.user_id = $2 AND f.is_deleted = false",
        file_id,
        claims.sub
    )
    .fetch_optional(&state.db_pool)
    .await?;

    let file = match file {
        Some(f) => f,
        None => return Err(AppError::NotFound("File not found".to_string())),
    };

    // 从存储后端下载文件内容
    let content = state.storage_manager.download_file(
        &file.storage_key, 
        &file.storage_type, 
        Some(claims.sub)
    ).await?;

    // 创建响应
    let response = axum::response::Response::builder()
        .header("Content-Type", file.mime_type.unwrap_or("application/octet-stream"))
        .header("Content-Length", file.size)
        .header("Content-Disposition", format!("attachment; filename=\"{}\"", file.name))
        .body(content)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(response)
}

pub async fn list_file_versions(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Path(file_id): Path<Uuid>,
) -> Result<Json<Vec<FileVersion>>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    // 验证文件属于当前用户
    let _ = get_file_info_internal(&state.db_pool, file_id, claims.sub).await?;

    let versions = sqlx::query_as!(
        FileVersion,
        "SELECT id, file_id, version_number, size, uploaded_by, uploaded_by_username, created_at FROM file_version_history WHERE file_id = $1 ORDER BY version_number DESC",
        file_id
    )
    .fetch_all(&state.db_pool)
    .await?;

    Ok(Json(versions))
}

pub async fn search_files(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Query(query): Query<SearchQuery>,
) -> Result<Json<FileListResponse>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20);
    let offset = (page - 1) * page_size;

    let (files, total) = sqlx::query_as::<(Uuid, String, String, i64, Option<String>, Option<Uuid>, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, bool, Option<chrono::DateTime<chrono::Utc>>, i64), _>(
        r#"
            SELECT 
                f.id, f.name, f.type, f.size, f.mime_type, f.parent_id,
                f.created_at, f.updated_at, f.is_deleted, f.trashed_at,
                COUNT(*) OVER() as total
            FROM files f
            WHERE f.user_id = $1 
                AND f.is_deleted = false
                AND f.name ILIKE $2
            ORDER BY f.name ASC
            LIMIT $3 OFFSET $4
        "#,
        claims.sub,
        format!("%{}%", query.q),
        page_size as i64,
        offset as i64
    )
    .fetch_all(&state.db_pool)
    .await?;

    let file_list: Vec<FileInfo> = files
        .into_iter()
        .map(|(id, name, type_, size, mime_type, parent_id, created_at, updated_at, is_deleted, trashed_at, _total)| {
            FileInfo {
                id,
                name,
                type_,
                size: size as u64,
                mime_type,
                parent_id,
                created_at,
                updated_at,
                is_deleted,
                trashed_at,
            }
        })
        .collect();

    let total = file_list.first().map(|_| file_list.len() as u64).unwrap_or(0);

    Ok(Json(FileListResponse {
        files: file_list,
        total,
        page,
        page_size,
    }))
}

// 离线下载功能
pub async fn create_offline_download(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Json(payload): Json<CreateOfflineDownloadRequest>,
) -> Result<Json<OfflineDownloadTask>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    let task_id = sqlx::query_scalar!(
        r#"
            INSERT INTO offline_download_tasks (user_id, source_url, file_name, status)
            VALUES ($1, $2, $3, 'pending')
            RETURNING id
        "#,
        claims.sub,
        payload.source_url,
        payload.file_name
    )
    .fetch_one(&state.db_pool)
    .await?;

    // 这里应该触发异步任务，例如使用 tokio::spawn 启动 aria2 下载
    // 简化处理：直接返回任务

    let task = sqlx::query_as!(
        OfflineDownloadTask,
        "SELECT id, user_id, source_url, file_name, status, progress, error_message, created_at, updated_at FROM offline_download_tasks WHERE id = $1",
        task_id
    )
    .fetch_one(&state.db_pool)
    .await?;

    Ok(Json(task))
}

pub async fn list_offline_downloads(
    State(state): State<AppState>,
    auth: AuthExtractor,
) -> Result<Json<Vec<OfflineDownloadTask>>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    let tasks = sqlx::query_as!(
        OfflineDownloadTask,
        "SELECT id, user_id, source_url, file_name, status, progress, error_message, created_at, updated_at FROM offline_download_tasks WHERE user_id = $1 ORDER BY created_at DESC",
        claims.sub
    )
    .fetch_all(&state.db_pool)
    .await?;

    Ok(Json(tasks))
}

pub async fn cancel_offline_download(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Path(task_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    sqlx::query!(
        "UPDATE offline_download_tasks SET status = 'cancelled' WHERE id = $1 AND user_id = $2",
        task_id,
        claims.sub
    )
    .execute(&state.db_pool)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

// 辅助函数
async fn create_file_record(
    db_pool: &PgPool,
    user_id: Uuid,
    name: &str,
    hash: &str,
    size: i64,
    storage_policy_id: Uuid,
    mime_type: Option<String>,
) -> Result<Uuid, AppError> {
    // 先查找是否已有相同哈希的文件（秒传）
    let existing = sqlx::query_scalar!(
        "SELECT f.id FROM files f JOIN file_blobs fb ON f.blob_id = fb.id WHERE fb.hash = $1 AND f.user_id = $2 AND f.is_deleted = false LIMIT 1",
        hash,
        user_id
    )
    .fetch_optional(db_pool)
    .await?;

    if let Some(existing_id) = existing {
        // 增加引用计数
        sqlx::query!(
            "UPDATE file_blobs SET ref_count = ref_count + 1 WHERE id = (SELECT blob_id FROM files WHERE id = $1)",
            existing_id
        )
        .execute(db_pool)
        .await?;

        return Ok(existing_id);
    }

    // 创建新文件记录
    let file_id = sqlx::query_scalar!(
        r#"
            INSERT INTO files (user_id, name, type, blob_id, size, mime_type, storage_policy_id)
            VALUES ($1, $2, 'file', (SELECT id FROM file_blobs WHERE hash = $3), $4, $5, $6)
            RETURNING id
        "#,
        user_id,
        name,
        hash,
        size,
        mime_type,
        storage_policy_id
    )
    .fetch_one(db_pool)
    .await?;

    Ok(file_id)
}

async fn create_file_version(
    db_pool: &PgPool,
    file_id: Uuid,
    version_number: i32,
    size: i64,
    uploaded_by: Uuid,
) -> Result<(), AppError> {
    // 获取下一个版本号
    let next_version = sqlx::query_scalar!(
        "SELECT COALESCE(MAX(version_number), 0) + 1 FROM file_versions WHERE file_id = $1",
        file_id
    )
    .fetch_one(db_pool)
    .await?;

    // 获取 blob_id
    let blob_id = sqlx::query_scalar!(
        "SELECT blob_id FROM files WHERE id = $1",
        file_id
    )
    .fetch_one(db_pool)
    .await?;

    sqlx::query!(
        r#"
            INSERT INTO file_versions (file_id, blob_id, version_number, size, uploaded_by, upload_ip)
            VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        file_id,
        blob_id,
        next_version,
        size,
        uploaded_by,
        None::<ipnetwork::IpNetwork>
    )
    .execute(db_pool)
    .await?;

    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct CreateOfflineDownloadRequest {
    pub source_url: String,
    pub file_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OfflineDownloadTask {
    pub id: Uuid,
    pub user_id: Uuid,
    pub source_url: String,
    pub file_name: Option<String>,
    pub status: String,
    pub progress: i32,
    pub error_message: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

// 错误类型
#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Unauthorized(String),
    NotFound(String),
    Internal(String),
    Database(sqlx::Error),
    Storage(StorageError),
    Jwt(jsonwebtoken::errors::Error),
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Database(err)
    }
}

impl From<StorageError> for AppError {
    fn from(err: StorageError) -> Self {
        AppError::Storage(err)
    }
}

impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        AppError::Jwt(err)
    }
}

impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            AppError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string()),
            AppError::Storage(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Storage error".to_string()),
            AppError::Jwt(_) => (StatusCode::UNAUTHORIZED, "Invalid token".to_string()),
        };

        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}
