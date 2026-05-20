use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use chrono::Utc;
use jsonwebtoken::{encode, decoding::DecodingKey, encoding::EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    config::Config,
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: UserResponse,
    pub expires_at: i64,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub avatar_url: Option<String>,
    pub is_admin: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub username: String,
    pub is_admin: bool,
    pub exp: usize,
}

pub async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    // 验证输入
    if payload.username.len() < 3 || payload.username.len() > 50 {
        return Err(AppError::BadRequest("Username must be between 3 and 50 characters".to_string()));
    }
    
    if payload.password.len() < 8 {
        return Err(AppError::BadRequest("Password must be at least 8 characters".to_string()));
    }

    // 检查用户是否已存在
    let existing_user = sqlx::query_as!(
        UserResponse,
        "SELECT id, username, email, avatar_url, is_admin FROM users WHERE email = $1 OR username = $2",
        payload.email,
        payload.username
    )
    .fetch_optional(&state.db_pool)
    .await?;

    if existing_user.is_some() {
        return Err(AppError::Conflict("User with this email or username already exists".to_string()));
    }

    // 哈希密码
    let password_hash = argon2::password_hash::PasswordHash::generate(
        argon2::Argon2::default(),
        payload.password.as_bytes(),
        &argon2::password_hash::SaltString::generate(&mut rand::thread_rng()),
        &argon2::password_hash::Params::default(),
    )?
    .to_string();

    // 创建用户
    let user_id = sqlx::query_scalar!(
        "INSERT INTO users (username, email, password_hash) VALUES ($1, $2, $3) RETURNING id",
        payload.username,
        payload.email,
        password_hash
    )
    .fetch_one(&state.db_pool)
    .await?;

    // 生成 JWT 令牌
    let token = generate_jwt(&user_id, &payload.username, false, &state.config.jwt.secret)?;
    let expires_at = get_expires_at(&state.config.jwt.expiration_hours);

    // 获取用户信息
    let user = sqlx::query_as!(
        UserResponse,
        "SELECT id, username, email, avatar_url, is_admin FROM users WHERE id = $1",
        user_id
    )
    .fetch_one(&state.db_pool)
    .await?;

    Ok(Json(AuthResponse {
        token,
        user,
        expires_at,
    }))
}

pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    // 查找用户
    let user = sqlx::query_as!(
        (Uuid, String, String, String),
        "SELECT id, username, email, password_hash FROM users WHERE email = $1 AND is_active = true",
        payload.email
    )
    .fetch_optional(&state.db_pool)
    .await?;

    let (user_id, username, email, password_hash) = match user {
        Some(data) => data,
        None => return Err(AppError::Unauthorized("Invalid credentials".to_string())),
    };

    // 验证密码
    let parsed_hash = argon2::password_hash::PasswordHash::new(&password_hash)
        .map_err(|_| AppError::Unauthorized("Invalid credentials".to_string()))?;
    
    argon2::Argon2::default()
        .verify_password(
            payload.password.as_bytes(),
            &parsed_hash,
        )
        .map_err(|_| AppError::Unauthorized("Invalid credentials".to_string()))?;

    // 获取用户信息
    let user = sqlx::query_as!(
        UserResponse,
        "SELECT id, username, email, avatar_url, is_admin FROM users WHERE id = $1",
        user_id
    )
    .fetch_one(&state.db_pool)
    .await?;

    // 生成 JWT 令牌
    let token = generate_jwt(&user_id, &username, user.is_admin, &state.config.jwt.secret)?;
    let expires_at = get_expires_at(&state.config.jwt.expiration_hours);

    Ok(Json(AuthResponse {
        token,
        user,
        expires_at,
    }))
}

pub async fn refresh_token(
    State(state): State<AppState>,
    Json(payload): Json<RefreshTokenRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    let claims = validate_jwt(&payload.token, &state.config.jwt.secret)?;
    
    // 获取用户信息
    let user = sqlx::query_as!(
        UserResponse,
        "SELECT id, username, email, avatar_url, is_admin FROM users WHERE id = $1 AND is_active = true",
        claims.sub
    )
    .fetch_one(&state.db_pool)
    .await?;

    // 生成新令牌
    let token = generate_jwt(&user.id, &user.username, user.is_admin, &state.config.jwt.secret)?;
    let expires_at = get_expires_at(&state.config.jwt.expiration_hours);

    Ok(Json(AuthResponse {
        token,
        user,
        expires_at,
    }))
}

pub async fn get_current_user(
    State(state): State<AppState>,
    auth: AuthExtractor,
) -> Result<Json<UserResponse>, AppError> {
    let claims = validate_jwt(&auth.token, &state.config.jwt.secret)?;
    
    let user = sqlx::query_as!(
        UserResponse,
        "SELECT id, username, email, avatar_url, is_admin FROM users WHERE id = $1 AND is_active = true",
        claims.sub
    )
    .fetch_one(&state.db_pool)
    .await?;

    Ok(Json(user))
}

// 辅助函数
fn generate_jwt(
    user_id: &Uuid,
    username: &str,
    is_admin: bool,
    secret: &str,
) -> Result<String, AppError> {
    let expiration = get_expires_at(&6); // 默认 6 小时
    let expiration_timestamp = expiration.timestamp() as usize;

    let claims = Claims {
        sub: *user_id,
        username: username.to_string(),
        is_admin,
        exp: expiration_timestamp,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;

    Ok(token)
}

fn validate_jwt(token: &str, secret: &str) -> Result<Claims, AppError> {
    let mut validation = Validation::default();
    validation.validate_exp = true;
    
    let token_data = jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?;

    Ok(token_data.claims)
}

fn get_expires_at(hours: &u64) -> chrono::DateTime<Utc> {
    Utc::now() + chrono::Duration::hours(*hours as i64)
}

// 中间件提取器
pub struct AuthExtractor {
    pub token: String,
}

impl axum::extract::FromRequestParts<AppState> for AuthExtractor {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut axum::extract::RequestParts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers()
            .get("Authorization")
            .and_then(|h| h.to_str().ok());

        let token = match auth_header {
            Some(header) if header.starts_with("Bearer ") => {
                header.strip_prefix("Bearer ").unwrap().to_string()
            }
            _ => return Err(AppError::Unauthorized("Missing or invalid authorization header".to_string())),
        };

        Ok(AuthExtractor { token })
    }
}

#[derive(Debug, Deserialize)]
pub struct RefreshTokenRequest {
    pub token: String,
}

// 错误类型
#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Unauthorized(String),
    Conflict(String),
    Database(sqlx::Error),
    Jwt(jsonwebtoken::errors::Error),
    PasswordHash(argon2::password_hash::Error),
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

impl From<argon2::password_hash::Error> for AppError {
    fn from(err: argon2::password_hash::Error) -> Self {
        AppError::PasswordHash(err)
    }
}

impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg),
            AppError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string()),
            AppError::Jwt(_) => (StatusCode::UNAUTHORIZED, "Invalid token".to_string()),
            AppError::PasswordHash(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Password error".to_string()),
        };

        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}
