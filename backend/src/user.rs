use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    auth::AuthExtractor,
    AppState,
};

#[derive(Debug, Serialize)]
pub struct UserProfileResponse {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub avatar_url: Option<String>,
    pub quota_size: i64,
    pub quota_used: i64,
    pub is_admin: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub username: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct QuotaInfoResponse {
    pub total: i64,
    pub used: i64,
    pub available: i64,
    pub usage_percentage: f64,
    pub file_count: i64,
    pub folder_count: i64,
}

#[derive(Debug, Serialize)]
pub struct WebDavTokenResponse {
    pub id: Uuid,
    pub token: String,
    pub description: Option<String>,
    pub is_active: bool,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct WebDavUsageResponse {
    pub total_requests: i64,
    pub upload_count: i64,
    pub download_count: i64,
    pub total_upload_bytes: i64,
    pub total_download_bytes: i64,
    pub last_access_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWebDavTokenRequest {
    pub description: Option<String>,
    pub expires_in_hours: Option<i64>,
}

pub async fn get_profile(
    State(state): State<AppState>,
    auth: AuthExtractor,
) -> Result<Json<UserProfileResponse>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    let user = sqlx::query_as!(
        UserProfileResponse,
        "SELECT id, username, email, avatar_url, quota_size, quota_used, is_admin, created_at FROM users WHERE id = $1",
        claims.sub
    )
    .fetch_one(&state.db_pool)
    .await?;

    Ok(Json(user))
}

pub async fn update_profile(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Json(payload): Json<UpdateProfileRequest>,
) -> Result<Json<UserProfileResponse>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    // 构建更新查询
    let mut updates = Vec::new();
    let mut values: Vec<&dyn sqlx::Encode<'_, sqlx::Postgres>> = vec![&claims.sub];

    if let Some(username) = payload.username {
        if username.len() < 3 || username.len() > 50 {
            return Err(AppError::BadRequest("Username must be between 3 and 50 characters".to_string()));
        }

        let existing = sqlx::query!(
            "SELECT id FROM users WHERE username = $1 AND id != $2",
            username,
            claims.sub
        )
        .fetch_optional(&state.db_pool)
        .await?;

        if existing.is_some() {
            return Err(AppError::Conflict("Username already taken".to_string()));
        }

        updates.push("username = $".to_string() + &(values.len() + 1).to_string());
        values.push(&username);
    }

    if let Some(avatar_url) = payload.avatar_url {
        updates.push("avatar_url = $".to_string() + &(values.len() + 1).to_string());
        values.push(&avatar_url);
    }

    if updates.is_empty() {
        return Err(AppError::BadRequest("No fields to update".to_string()));
    }

    let query = format!(
        "UPDATE users SET {} WHERE id = ${}",
        updates.join(", "),
        values.len() + 1
    );

    sqlx::query(&query)
        .bind(&values[..])
        .execute(&state.db_pool)
        .await?;

    let user = sqlx::query_as!(
        UserProfileResponse,
        "SELECT id, username, email, avatar_url, quota_size, quota_used, is_admin, created_at FROM users WHERE id = $1",
        claims.sub
    )
    .fetch_one(&state.db_pool)
    .await?;

    Ok(Json(user))
}

pub async fn get_quota(
    State(state): State<AppState>,
    auth: AuthExtractor,
) -> Result<Json<QuotaInfoResponse>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    let user = sqlx::query!(
        "SELECT quota_size, quota_used FROM users WHERE id = $1",
        claims.sub
    )
    .fetch_one(&state.db_pool)
    .await?;

    let stats = sqlx::query!(
        r#"
            SELECT 
                COUNT(*) FILTER (WHERE type = 'file' AND is_deleted = false) as file_count,
                COUNT(*) FILTER (WHERE type = 'folder' AND is_deleted = false) as folder_count
            FROM files
            WHERE user_id = $1
        "#,
        claims.sub
    )
    .fetch_one(&state.db_pool)
    .await?;

    let total = user.quota_size;
    let used = user.quota_used;
    let available = total - used;
    let usage_percentage = if total > 0 {
        (used as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    Ok(Json(QuotaInfoResponse {
        total,
        used,
        available,
        usage_percentage,
        file_count: stats.file_count.unwrap_or(0),
        folder_count: stats.folder_count.unwrap_or(0),
    }))
}

/// 获取 WebDAV 令牌列表
pub async fn list_webdav_tokens(
    State(state): State<AppState>,
    auth: AuthExtractor,
) -> Result<Json<Vec<WebDavTokenResponse>>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    let tokens = sqlx::query_as!(
        WebDavTokenResponse,
        "SELECT id, token, description, is_active, last_used_at, expires_at, created_at FROM webdav_tokens WHERE user_id = $1 ORDER BY created_at DESC",
        claims.sub
    )
    .fetch_all(&state.db_pool)
    .await?;

    Ok(Json(tokens))
}

/// 创建 WebDAV 令牌
pub async fn create_webdav_token(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Json(payload): Json<CreateWebDavTokenRequest>,
) -> Result<Json<WebDavTokenResponse>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    // 生成随机令牌
    let token = generate_webdav_token();

    // 计算过期时间
    let expires_at = payload.expires_in_hours.map(|hours| {
        chrono::Utc::now() + chrono::Duration::hours(hours)
    });

    let token_id = sqlx::query_scalar!(
        r#"
        INSERT INTO webdav_tokens (user_id, token, description, expires_at)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
        claims.sub,
        token,
        payload.description,
        expires_at
    )
    .fetch_one(&state.db_pool)
    .await?;

    let response = sqlx::query_as!(
        WebDavTokenResponse,
        "SELECT id, token, description, is_active, last_used_at, expires_at, created_at FROM webdav_tokens WHERE id = $1",
        token_id
    )
    .fetch_one(&state.db_pool)
    .await?;

    Ok(Json(response))
}

/// 删除 WebDAV 令牌
pub async fn delete_webdav_token(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Path(token_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    // 验证令牌属于当前用户
    let token = sqlx::query!(
        "SELECT id FROM webdav_tokens WHERE id = $1 AND user_id = $2",
        token_id,
        claims.sub
    )
    .fetch_optional(&state.db_pool)
    .await?;

    if token.is_none() {
        return Err(AppError::NotFound("WebDAV token not found".to_string()));
    }

    sqlx::query!(
        "DELETE FROM webdav_tokens WHERE id = $1 AND user_id = $2",
        token_id,
        claims.sub
    )
    .execute(&state.db_pool)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// 获取 WebDAV 使用统计
pub async fn get_webdav_usage(
    State(state): State<AppState>,
    auth: AuthExtractor,
) -> Result<Json<WebDavUsageResponse>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    let usage = sqlx::query_as!(
        (i64, i64, i64, i64, i64, Option<chrono::DateTime<chrono::Utc>>),
        r#"
        SELECT 
            COUNT(*) as total_requests,
            COUNT(*) FILTER (WHERE method = 'PUT') as upload_count,
            COUNT(*) FILTER (WHERE method = 'GET') as download_count,
            COALESCE(SUM(CASE WHEN method = 'PUT' THEN response_size ELSE 0 END), 0) as total_upload_bytes,
            COALESCE(SUM(CASE WHEN method = 'GET' THEN response_size ELSE 0 END), 0) as total_download_bytes,
            MAX(created_at) as last_access_at
        FROM webdav_access_logs
        WHERE user_id = $1
        "#,
        claims.sub
    )
    .fetch_one(&state.db_pool)
    .await?;

    Ok(Json(WebDavUsageResponse {
        total_requests: usage.0,
        upload_count: usage.1,
        download_count: usage.2,
        total_upload_bytes: usage.3,
        total_download_bytes: usage.4,
        last_access_at: usage.5,
    }))
}

// 其他函数（团队、回收站等）保持不变...
// 为简洁起见，这里省略其他函数，实际代码应包含所有之前的函数

fn generate_webdav_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let chars: Vec<char> = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".chars().collect();
    (0..64)
        .map(|_| rng.choose(&chars).unwrap())
        .collect()
}

// 错误类型
#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Unauthorized(String),
    Conflict(String),
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
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            AppError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string()),
            AppError::Jwt(_) => (StatusCode::UNAUTHORIZED, "Invalid token".to_string()),
        };

        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}
