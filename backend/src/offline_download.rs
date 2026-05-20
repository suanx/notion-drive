use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use url::Url;
use uuid::Uuid;

use crate::{
    auth::AuthExtractor,
    AppState,
};

/// 安全的 URL 验证 - 防止 SSRF 攻击
fn is_valid_url(url: &str) -> Result<(), String> {
    // 解析 URL
    let parsed = Url::parse(url)
        .map_err(|e| format!("Invalid URL format: {}", e))?;

    // 只允许 http, https, ftp 协议
    match parsed.scheme() {
        "http" | "https" | "ftp" => {},
        _ => return Err(format!("Unsupported protocol: {}", parsed.scheme())),
    }

    // 检查主机名
    if let Some(host) = parsed.host_str() {
        // 检查是否为 IP 地址
        if let Ok(ip) = host.parse::<IpAddr>() {
            // 拒绝私有 IP 地址（防止访问内网）
            if ip.is_private() {
                return Err("Cannot access private IP addresses".to_string());
            }
            // 拒绝回环地址
            if ip.is_loopback() {
                return Err("Cannot access loopback address".to_string());
            }
            // 拒绝链路本地地址
            if ip.is_link_local() {
                return Err("Cannot access link-local address".to_string());
            }
            // 拒绝多播地址
            if ip.is_multicast() {
                return Err("Cannot access multicast address".to_string());
            }
        }
    }

    Ok(())
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

pub async fn create_offline_download(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Json(payload): Json<CreateOfflineDownloadRequest>,
) -> Result<Json<OfflineDownloadTask>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    // 验证 URL 格式 - 防止 SSRF
    is_valid_url(&payload.source_url)
        .map_err(|e| AppError::BadRequest(e))?;

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

    // 触发异步下载任务
    // 在实际应用中，这里应该调用 aria2 或类似的下载工具
    // 简化处理：直接返回任务，实际下载由后台任务处理
    tokio::spawn(async move {
        process_offline_download(&state.db_pool, task_id, &payload.source_url).await;
    });

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

// 异步下载处理函数
async fn process_offline_download(
    db_pool: &sqlx::PgPool,
    task_id: Uuid,
    source_url: &str,
) {
    // 更新状态为下载中
    let _ = sqlx::query!(
        "UPDATE offline_download_tasks SET status = 'downloading', updated_at = $1 WHERE id = $2",
        chrono::Utc::now(),
        task_id
    )
    .execute(db_pool)
    .await;

    // 模拟下载过程（实际应该调用 aria2）
    // 这里简化处理：直接标记为完成
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // 更新状态为完成
    let _ = sqlx::query!(
        "UPDATE offline_download_tasks SET status = 'completed', progress = 100, updated_at = $1 WHERE id = $2",
        chrono::Utc::now(),
        task_id
    )
    .execute(db_pool)
    .await;

    // 实际应用中，下载完成后应该将文件上传到存储后端
    // 这里省略具体实现
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
