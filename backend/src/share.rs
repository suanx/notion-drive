use axum::{
    extract::{Path, Query, State},
    http::{Request, StatusCode},
    Json,
};
use chrono::Utc;
use governor::{DirectRateLimiter, Quota, StateRateLimiter};
use nonempty::nonempty;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    auth::AuthExtractor,
    file::FileInfo,
    AppState,
};

/// 全局分享访问速率限制器 - 每个 token 限制 10 次/分钟
static SHARE_RATE_LIMITER: RwLock<Arc<StateRateLimiter<String, governor::InMemoryState>>> = 
    RwLock::const_new(Arc::new(StateRateLimiter::new(Quota::per_minute(nonempty![10]))));

/// 检查分享访问速率限制
async fn check_share_rate_limit(token: &str) -> Result<(), StatusCode> {
    let limiter = SHARE_RATE_LIMITER.read().await;
    if limiter.check_key(token).is_err() {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    Ok(())
}

/// 哈希分享密码
fn hash_share_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let password_hash = argon2::password_hash::PasswordHash::generate(
        argon2::Argon2::default(),
        password.as_bytes(),
        &argon2::password_hash::SaltString::generate(&mut rand::thread_rng()),
        &argon2::password_hash::Params::default(),
    )?
    .to_string();
    Ok(password_hash)
}

/// 验证分享密码
fn verify_share_password(password: &str, stored_hash: &str) -> Result<bool, argon2::password_hash::Error> {
    let parsed_hash = argon2::password_hash::PasswordHash::new(stored_hash)?;
    argon2::Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .map(|_| true)
        .map_err(|e| e.into())
}

#[derive(Debug, Deserialize)]
pub struct CreateShareRequest {
    pub file_id: Uuid,
    pub password: Option<String>,
    pub expires_at: Option<chrono::DateTime<Utc>>,
    pub max_downloads: Option<i32>,
    pub allow_preview: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ListSharesQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ShareLinkResponse {
    pub id: Uuid,
    pub file_id: Uuid,
    pub file: Option<FileInfo>,
    pub token: String,
    pub password: Option<String>,
    pub expires_at: Option<chrono::DateTime<Utc>>,
    pub max_downloads: Option<i32>,
    pub download_count: i32,
    pub allow_preview: bool,
    pub share_url: String,
    pub created_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ShareInfoResponse {
    pub id: Uuid,
    pub file: FileInfo,
    pub allow_preview: bool,
    pub requires_password: bool,
    pub expires_at: Option<chrono::DateTime<Utc>>,
    pub remaining_downloads: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct SharePasswordRequest {
    pub password: String,
}

pub async fn create_share(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Json(payload): Json<CreateShareRequest>,
) -> Result<Json<ShareLinkResponse>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    // 验证文件是否存在且属于当前用户
    let file = sqlx::query!(
        "SELECT id, name, type, size, mime_type, parent_id FROM files WHERE id = $1 AND user_id = $2 AND is_deleted = false",
        payload.file_id,
        claims.sub
    )
    .fetch_optional(&state.db_pool)
    .await?;

    let file = match file {
        Some(f) => f,
        None => return Err(AppError::NotFound("File not found".to_string())),
    };

    // 生成分享链接 token
    let token = generate_share_token();

    // 哈希密码（如果提供）
    let password_hash = payload.password.as_ref().map(|p| hash_share_password(p)).transpose()
        .map_err(|e| AppError::Internal(format!("Failed to hash password: {}", e)))?;

    // 创建分享链接
    let share_id = sqlx::query_scalar!(
        r#"
            INSERT INTO share_links (file_id, user_id, token, password, expires_at, max_downloads, allow_preview)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
        "#,
        payload.file_id,
        claims.sub,
        token,
        password_hash,
        payload.expires_at,
        payload.max_downloads,
        payload.allow_preview.unwrap_or(true)
    )
    .fetch_one(&state.db_pool)
    .await?;

    let base_url = state.config.server.base_url
        .unwrap_or_else(|| "http://localhost:8080".to_string());
    let share_url = format!("{}/api/v1/shares/public/{}", base_url, token);

    // 查询时不返回密码哈希
    let share = sqlx::query!(
        "SELECT id, file_id, token, expires_at, max_downloads, download_count, allow_preview, created_at FROM share_links WHERE id = $1",
        share_id
    )
    .fetch_one(&state.db_pool)
    .await?;

    Ok(Json(ShareLinkResponse {
        id: share.id,
        file_id: share.file_id,
        file: Some(FileInfo {
            id: file.id,
            name: file.name,
            type_: file.type_,
            size: file.size as u64,
            mime_type: file.mime_type,
            parent_id: file.parent_id,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            is_deleted: false,
        }),
        token: share.token,
        password: payload.password.map(|_| "[set]".to_string()), // 不暴露实际密码
        expires_at: share.expires_at,
        max_downloads: share.max_downloads,
        download_count: share.download_count,
        allow_preview: share.allow_preview,
        share_url,
        created_at: share.created_at,
    }))
}

pub async fn list_shares(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Query(query): Query<ListSharesQuery>,
) -> Result<Json<Vec<ShareLinkResponse>>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20);
    let offset = (page - 1) * page_size;

    // 不返回密码哈希
    let shares = sqlx::query_as!(
        ShareLinkResponse,
        r#"
            SELECT 
                sl.id, sl.file_id, sl.token, NULL as password, sl.expires_at, 
                sl.max_downloads, sl.download_count, sl.allow_preview, sl.created_at
            FROM share_links sl
            WHERE sl.user_id = $1
            ORDER BY sl.created_at DESC
            LIMIT $2 OFFSET $3
        "#,
        claims.sub,
        page_size as i64,
        offset as i64
    )
    .fetch_all(&state.db_pool)
    .await?;

    Ok(Json(shares))
}

pub async fn delete_share(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Path(share_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    // 验证分享链接属于当前用户
    let share = sqlx::query!(
        "SELECT id FROM share_links WHERE id = $1 AND user_id = $2",
        share_id,
        claims.sub
    )
    .fetch_optional(&state.db_pool)
    .await?;

    if share.is_none() {
        return Err(AppError::NotFound("Share link not found".to_string()));
    }

    sqlx::query!(
        "DELETE FROM share_links WHERE id = $1",
        share_id
    )
    .execute(&state.db_pool)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_share_info(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(query): Option<Json<SharePasswordRequest>>,
) -> Result<Json<ShareInfoResponse>, AppError> {
    // 检查速率限制
    check_share_rate_limit(&token).await
        .map_err(|_| AppError::Unauthorized("Too many requests".to_string()))?;

    // 查找分享链接
    let share = sqlx::query!(
        "SELECT id, file_id, password, expires_at, max_downloads, download_count, allow_preview FROM share_links WHERE token = $1",
        token
    )
    .fetch_optional(&state.db_pool)
    .await?;

    let share = match share {
        Some(s) => s,
        None => return Err(AppError::NotFound("Share link not found".to_string())),
    };

    // 检查是否过期
    if let Some(expires_at) = share.expires_at {
        if Utc::now() > expires_at {
            return Err(AppError::Unauthorized("Share link has expired".to_string()));
        }
    }

    // 检查下载次数限制
    if let Some(max) = share.max_downloads {
        if share.download_count >= max {
            return Err(AppError::Unauthorized("Download limit reached".to_string()));
        }
    }

    // 验证密码哈希
    if let Some(stored_hash) = share.password {
        let provided_password = query
            .ok_or(AppError::Unauthorized("Password required".to_string()))?
            .password;
        
        verify_share_password(&provided_password, &stored_hash)
            .map_err(|_| AppError::Unauthorized("Invalid password".to_string()))?
            .then_some(())
            .ok_or(AppError::Unauthorized("Invalid password".to_string()))?;
    }

    // 获取文件信息
    let file = sqlx::query_as!(
        FileInfo,
        "SELECT id, name, type as type_, size, mime_type, parent_id, created_at, updated_at, is_deleted FROM files WHERE id = $1 AND is_deleted = false",
        share.file_id
    )
    .fetch_one(&state.db_pool)
    .await?;

    Ok(Json(ShareInfoResponse {
        id: share.id,
        file,
        allow_preview: share.allow_preview,
        requires_password: share.password.is_some(),
        expires_at: share.expires_at,
        remaining_downloads: share.max_downloads.map(|max| max - share.download_count),
    }))
}

pub async fn download_share(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(query): Option<Json<SharePasswordRequest>>,
) -> Result<axum::response::Response, AppError> {
    // 检查速率限制
    check_share_rate_limit(&token).await
        .map_err(|_| AppError::Unauthorized("Too many requests".to_string()))?;

    // 验证分享链接
    let share = sqlx::query!(
        "SELECT id, file_id, password, expires_at, max_downloads, download_count FROM share_links WHERE token = $1",
        token
    )
    .fetch_optional(&state.db_pool)
    .await?;

    let share = match share {
        Some(s) => s,
        None => return Err(AppError::NotFound("Share link not found".to_string())),
    };

    // 检查过期
    if let Some(expires_at) = share.expires_at {
        if Utc::now() > expires_at {
            return Err(AppError::Unauthorized("Share link has expired".to_string()));
        }
    }

    // 检查下载限制
    if let Some(max) = share.max_downloads {
        if share.download_count >= max {
            return Err(AppError::Unauthorized("Download limit reached".to_string()));
        }
    }

    // 验证密码哈希
    if let Some(stored_hash) = share.password {
        let provided_password = query
            .ok_or(AppError::Unauthorized("Password required".to_string()))?
            .password;
        
        verify_share_password(&provided_password, &stored_hash)
            .map_err(|_| AppError::Unauthorized("Invalid password".to_string()))?
            .then_some(())
            .ok_or(AppError::Unauthorized("Invalid password".to_string()))?;
    }

    // 获取文件信息
    let file = sqlx::query!(
        "SELECT fb.storage_key, fb.size, f.name, f.mime_type FROM files f JOIN file_blobs fb ON f.blob_id = fb.id WHERE f.id = $1 AND f.is_deleted = false",
        share.file_id
    )
    .fetch_optional(&state.db_pool)
    .await?;

    let file = match file {
        Some(f) => f,
        None => return Err(AppError::NotFound("File not found".to_string())),
    };

    // 下载文件内容
    let content = state.storage_manager.download_file(&file.storage_key).await?;

    // 增加下载计数
    sqlx::query!(
        "UPDATE share_links SET download_count = download_count + 1 WHERE id = $1",
        share.id
    )
    .execute(&state.db_pool)
    .await?;

    // 创建响应
    let response = axum::response::Response::builder()
        .header("Content-Type", file.mime_type.unwrap_or("application/octet-stream"))
        .header("Content-Length", file.size)
        .header("Content-Disposition", format!("attachment; filename=\"{}\"", file.name))
        .body(content)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(response)
}

fn generate_share_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let chars: Vec<char> = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".chars().collect();
    (0..32)
        .map(|_| rng.choose(&chars).unwrap())
        .collect()
}

// 错误类型
#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Unauthorized(String),
    NotFound(String),
    Internal(String),
    Database(sqlx::Error),
    Jwt(jsonwebtoken::errors::Error),
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Database(err)
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
            AppError::Jwt(_) => (StatusCode::UNAUTHORIZED, "Invalid token".to_string()),
        };

        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}
