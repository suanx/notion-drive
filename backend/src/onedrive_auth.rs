use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::AuthExtractor,
    storage::onedrive::{OneDriveStorage, OneDriveConfig},
    AppState,
};

#[derive(Debug, Serialize)]
pub struct AuthorizationUrlResponse {
    pub authorization_url: String,
    pub state: String,
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: String,
    pub state: String,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TokenStatusResponse {
    pub connected: bool,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub drive_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DisconnectResponse {
    pub success: bool,
    pub message: String,
}

/// 获取 OneDrive 授权 URL
pub async fn get_onedrive_authorization_url(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Query(query): Query<OneDrivePolicyQuery>,
) -> Result<Json<AuthorizationUrlResponse>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    // 查找 OneDrive 存储策略
    let policy = sqlx::query!(
        "SELECT id, config FROM storage_policies WHERE driver = 'onedrive' AND id = $1",
        query.policy_id
    )
    .fetch_optional(&state.db_pool)
    .await?;

    let policy = match policy {
        Some(p) => p,
        None => return Err(AppError::NotFound("OneDrive storage policy not found".to_string())),
    };

    let config: serde_json::Value = serde_json::from_str(&policy.config)
        .map_err(|_| AppError::BadRequest("Invalid OneDrive configuration".to_string()))?;

    let onedrive_config = OneDriveConfig {
        client_id: config["client_id"].as_str()
            .ok_or(AppError::BadRequest("Missing client_id".to_string()))?
            .to_string(),
        client_secret: config["client_secret"].as_str().map(String::from),
        tenant: config["tenant"].as_str()
            .ok_or(AppError::BadRequest("Missing tenant".to_string()))?
            .to_string(),
        drive_type: config["drive_type"].as_str()
            .ok_or(AppError::BadRequest("Missing drive_type".to_string()))?
            .to_string(),
        redirect_uri: config["redirect_uri"].as_str().map(String::from),
    };

    let onedrive = OneDriveStorage::new(onedrive_config, state.db_pool.clone());
    
    // 生成 CSRF 保护状态
    let state = generate_state();

    // 保存授权状态
    save_authorization_state(&state.db_pool, claims.sub, "onedrive", &state).await?;

    let authorization_url = onedrive.get_authorization_url(claims.sub, &state)?;

    Ok(Json(AuthorizationUrlResponse {
        authorization_url,
        state,
    }))
}

/// 处理 OneDrive 授权回调
pub async fn onedrive_callback(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Path(policy_id): Path<Uuid>,
    Query(query): Query<CallbackQuery>,
) -> Result<Json<TokenStatusResponse>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    // 检查是否有错误
    if let Some(error) = query.error {
        return Err(AppError::BadRequest(format!("Authorization failed: {}", error)));
    }

    // 验证 state（CSRF 保护）
    let saved_state = sqlx::query!(
        "SELECT state FROM oauth_authorization_states WHERE user_id = $1 AND provider = 'onedrive' AND expires_at > NOW()",
        claims.sub
    )
    .fetch_optional(&state.db_pool)
    .await?;

    if saved_state.is_none() || saved_state.as_ref().unwrap().state != query.state {
        return Err(AppError::BadRequest("Invalid state parameter".to_string()));
    }

    // 查找 OneDrive 存储策略
    let policy = sqlx::query!(
        "SELECT config FROM storage_policies WHERE id = $1 AND driver = 'onedrive'",
        policy_id
    )
    .fetch_optional(&state.db_pool)
    .await?;

    let policy = match policy {
        Some(p) => p,
        None => return Err(AppError::NotFound("OneDrive storage policy not found".to_string())),
    };

    let config: serde_json::Value = serde_json::from_str(&policy.config)
        .map_err(|_| AppError::BadRequest("Invalid OneDrive configuration".to_string()))?;

    let onedrive_config = OneDriveConfig {
        client_id: config["client_id"].as_str()
            .ok_or(AppError::BadRequest("Missing client_id".to_string()))?
            .to_string(),
        client_secret: config["client_secret"].as_str().map(String::from),
        tenant: config["tenant"].as_str()
            .ok_or(AppError::BadRequest("Missing tenant".to_string()))?
            .to_string(),
        drive_type: config["drive_type"].as_str()
            .ok_or(AppError::BadRequest("Missing drive_type".to_string()))?
            .to_string(),
        redirect_uri: config["redirect_uri"].as_str().map(String::from),
    };

    let onedrive = OneDriveStorage::new(onedrive_config, state.db_pool.clone());
    
    // 交换授权码为令牌
    onedrive.exchange_code_for_token(claims.sub, &query.code).await?;

    // 清除授权状态
    sqlx::query!(
        "DELETE FROM oauth_authorization_states WHERE user_id = $1 AND provider = 'onedrive'",
        claims.sub
    )
    .execute(&state.db_pool)
    .await?;

    // 返回令牌状态
    let token_status = get_onedrive_token_status(&state.db_pool, claims.sub).await?;

    Ok(Json(token_status))
}

/// 获取 OneDrive 令牌状态
pub async fn get_onedrive_token_status(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Path(policy_id): Path<Uuid>,
) -> Result<Json<TokenStatusResponse>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    let token_status = get_onedrive_token_status(&state.db_pool, claims.sub).await?;

    Ok(Json(token_status))
}

/// 断开 OneDrive 连接
pub async fn disconnect_onedrive(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Path(policy_id): Path<Uuid>,
) -> Result<Json<DisconnectResponse>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    // 删除令牌
    sqlx::query!(
        "DELETE FROM onedrive_tokens WHERE user_id = $1",
        claims.sub
    )
    .execute(&state.db_pool)
    .await?;

    // 删除授权状态
    sqlx::query!(
        "DELETE FROM oauth_authorization_states WHERE user_id = $1 AND provider = 'onedrive'",
        claims.sub
    )
    .execute(&state.db_pool)
    .await?;

    Ok(Json(DisconnectResponse {
        success: true,
        message: "OneDrive disconnected successfully".to_string(),
    }))
}

// 辅助函数
async fn save_authorization_state(
    db_pool: &sqlx::PgPool,
    user_id: Uuid,
    provider: &str,
    state: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        INSERT INTO oauth_authorization_states (user_id, provider, state, expires_at)
        VALUES ($1, $2, $3, NOW() + INTERVAL '10 minutes')
        ON CONFLICT (user_id, provider) DO UPDATE SET
            state = $3,
            expires_at = NOW() + INTERVAL '10 minutes'
        "#,
        user_id,
        provider,
        state
    )
    .execute(db_pool)
    .await?;

    Ok(())
}

async fn get_onedrive_token_status(
    db_pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<TokenStatusResponse, AppError> {
    let token = sqlx::query!(
        "SELECT expires_at FROM onedrive_tokens WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1",
        user_id
    )
    .fetch_optional(db_pool)
    .await?;

    let connected = token.is_some();
    let expires_at = token.map(|t| t.expires_at);

    Ok(TokenStatusResponse {
        connected,
        expires_at,
        drive_type: None, // 可以从令牌中解析
    })
}

fn generate_state() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let chars: Vec<char> = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".chars().collect();
    (0..32)
        .map(|_| rng.choose(&chars).unwrap())
        .collect()
}

#[derive(Debug, Deserialize)]
pub struct OneDrivePolicyQuery {
    pub policy_id: Uuid,
}

// 错误类型
#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Unauthorized(String),
    NotFound(String),
    Internal(String),
    Database(sqlx::Error),
    Storage(crate::storage::StorageError),
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
            AppError::Storage(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Storage error".to_string()),
            AppError::Jwt(_) => (StatusCode::UNAUTHORIZED, "Invalid token".to_string()),
        };

        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}
