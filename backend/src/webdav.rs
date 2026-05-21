use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    auth::AuthExtractor,
    storage::StorageManager,
    AppState,
};

/// WebDAV 服务器状态
pub struct WebDavState {
    pub pool: PgPool,
    pub storage_manager: StorageManager,
    pub base_url: String,
}

/// WebDAV 请求上下文
pub struct WebDavContext {
    pub user_id: Uuid,
    pub username: String,
    pub is_admin: bool,
    pub token: Option<String>,
}

/// WebDAV 资源类型
#[derive(Debug, Clone, Serialize)]
pub enum WebDavResourceType {
    Collection, // 文件夹
    File,       // 文件
}

/// WebDAV 资源信息
#[derive(Debug, Clone, Serialize)]
pub struct WebDavResource {
    pub href: String,
    pub resourcetype: WebDavResourceType,
    pub getcontentlength: Option<u64>,
    pub getcontenttype: Option<String>,
    pub getlastmodified: DateTime<Utc>,
    pub creationdate: DateTime<Utc>,
    pub displayname: String,
    pub lockdiscovery: Option<Vec<LockInfo>>,
    pub supportedlock: Option<Vec<LockType>>,
}

/// 锁信息
#[derive(Debug, Clone, Serialize)]
pub struct LockInfo {
    pub activelock: ActiveLock,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActiveLock {
    pub lockscope: String, // exclusive or shared
    pub locktype: String,  // write or read
    pub depth: String,     // 0, 1, infinity
    pub owner: Option<String>,
    pub timeout: String,   // Second-3600
    pub locktoken: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LockType {
    pub lockscope: String,
    pub locktype: String,
}

/// WebDAV PROPFIND 请求体
#[derive(Debug, Deserialize)]
pub struct PropFindRequest {
    pub propfind: Option<PropFindBody>,
}

#[derive(Debug, Deserialize)]
pub struct PropFindBody {
    pub prop: Option<Prop>,
    pub allprop: Option<bool>,
    pub propname: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct Prop {
    #[serde(rename = "getcontentlength")]
    pub getcontentlength: Option<bool>,
    #[serde(rename = "getcontenttype")]
    pub getcontenttype: Option<bool>,
    #[serde(rename = "getlastmodified")]
    pub getlastmodified: Option<bool>,
    #[serde(rename = "creationdate")]
    pub creationdate: Option<bool>,
    #[serde(rename = "displayname")]
    pub displayname: Option<bool>,
    #[serde(rename = "resourcetype")]
    pub resourcetype: Option<bool>,
    #[serde(rename = "lockdiscovery")]
    pub lockdiscovery: Option<bool>,
    #[serde(rename = "getetag")]
    pub getetag: Option<bool>,
}

/// WebDAV MKCOL 请求体
#[derive(Debug, Deserialize)]
pub struct MkColRequest {
    pub set: Option<Set>,
}

#[derive(Debug, Deserialize)]
pub struct Set {
    pub prop: Prop,
}

/// WebDAV LOCK 请求体
#[derive(Debug, Deserialize)]
pub struct LockRequest {
    pub lockinfo: Option<LockInfoBody>,
}

#[derive(Debug, Deserialize)]
pub struct LockInfoBody {
    pub lockscope: LockScope,
    pub locktype: LockTypeBody,
    pub owner: Option<Owner>,
    pub timeout: Option<Timeout>,
}

#[derive(Debug, Deserialize)]
pub struct LockScope {
    #[serde(rename = "exclusive")]
    pub exclusive: Option<bool>,
    #[serde(rename = "shared")]
    pub shared: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct LockTypeBody {
    pub write: Option<bool>,
    pub read: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct Owner {
    pub href: Option<String>,
    pub displayname: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Timeout {
    #[serde(rename = "Second-")]
    pub second: Option<u64>,
}

/// WebDAV 响应
#[derive(Debug, Serialize)]
pub struct MultistatusResponse {
    pub response: Vec<PropFindResponse>,
}

#[derive(Debug, Serialize)]
pub struct PropFindResponse {
    pub href: String,
    pub propstat: Vec<PropStat>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PropStat {
    pub prop: WebDavResource,
    pub status: String,
}

/// WebDAV API 路由处理
pub async fn webdav_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request<Body>,
) -> Result<Response, AppError> {
    // 提取 WebDAV 认证信息
    let context = authenticate_webdav(&headers, &state.db_pool, &state.config.jwt.secret).await?;

    // 获取请求路径
    let path = request.uri().path().to_string();
    let method = request.method().to_string();

    // 记录访问日志
    let start_time = Utc::now();

    // 根据方法分发处理
    let response = match method.as_str() {
        "PROPFIND" => handle_propfind(state, context, &path, request).await,
        "GET" | "HEAD" => handle_get(state, context, &path).await,
        "PUT" => handle_put(state, context, &path, request).await,
        "DELETE" => handle_delete(state, context, &path).await,
        "MKCOL" => handle_mkcol(state, context, &path, request).await,
        "COPY" => handle_copy(state, context, &path, headers).await,
        "MOVE" => handle_move(state, context, &path, headers).await,
        "LOCK" => handle_lock(state, context, &path, request).await,
        "UNLOCK" => handle_unlock(state, context, &path, headers).await,
        "OPTIONS" => handle_options().await,
        "PROPPATCH" => handle_proppatch(state, context, &path, request).await,
        _ => Err(AppError::MethodNotAllowed(method)),
    };

    // 记录日志
    log_webdav_access(&state.db_pool, &context, &method, &path, &response, start_time).await;

    response
}

/// WebDAV 认证
async fn authenticate_webdav(
    headers: &HeaderMap,
    pool: &PgPool,
    jwt_secret: &str,
) -> Result<WebDavContext, AppError> {
    // 尝试 Bearer token（JWT）
    if let Some(auth_header) = headers.get("Authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if auth_str.starts_with("Bearer ") {
                let token = auth_str.strip_prefix("Bearer ").unwrap();
                let claims = crate::auth::validate_jwt(token, jwt_secret)?;
                
                let user = sqlx::query!(
                    "SELECT id, username, is_admin FROM users WHERE id = $1 AND is_active = true",
                    claims.sub
                )
                .fetch_one(pool)
                .await?;

                return Ok(WebDavContext {
                    user_id: user.id,
                    username: user.username,
                    is_admin: user.is_admin,
                    token: Some(token.to_string()),
                });
            }

            // 尝试 Basic 认证（WebDAV 客户端常用）
            if auth_str.starts_with("Basic ") {
                use base64::{engine::general_purpose, Engine as _};
                
                let encoded = auth_str.strip_prefix("Basic ").unwrap();
                let decoded = general_purpose::STANDARD.decode(encoded)
                    .map_err(|_| AppError::Unauthorized("Invalid Basic auth encoding".to_string()))?;
                
                let auth_str = String::from_utf8(decoded)
                    .map_err(|_| AppError::Unauthorized("Invalid Basic auth encoding".to_string()))?;
                
                let parts: Vec<&str> = auth_str.split(':').collect();
                if parts.len() != 2 {
                    return Err(AppError::Unauthorized("Invalid Basic auth format".to_string()));
                }

                let username = parts[0];
                let password = parts[1];

                // 验证用户密码
                let user = sqlx::query!(
                    "SELECT id, username, password_hash, is_admin FROM users WHERE username = $1 AND is_active = true",
                    username
                )
                .fetch_optional(pool)
                .await?;

                let user = match user {
                    Some(u) => u,
                    None => return Err(AppError::Unauthorized("Invalid credentials".to_string())),
                };

                // 验证密码
                let parsed_hash = argon2::password_hash::PasswordHash::new(&user.password_hash)
                    .map_err(|_| AppError::Unauthorized("Invalid credentials".to_string()))?;
                
                argon2::Argon2::default()
                    .verify_password(password.as_bytes(), &parsed_hash)
                    .map_err(|_| AppError::Unauthorized("Invalid credentials".to_string()))?;

                return Ok(WebDavContext {
                    user_id: user.id,
                    username: user.username,
                    is_admin: user.is_admin,
                    token: None,
                });
            }
        }
    }

    // 尝试 WebDAV token（查询参数或自定义头）
    if let Some(token) = headers.get("X-WebDav-Token").and_then(|h| h.to_str().ok()) {
        let token_data = sqlx::query!(
            "SELECT user_id FROM webdav_tokens WHERE token = $1 AND is_active = true",
            token
        )
        .fetch_optional(pool)
        .await?;

        if let Some(data) = token_data {
            let user = sqlx::query!(
                "SELECT id, username, is_admin FROM users WHERE id = $1 AND is_active = true",
                data.user_id
            )
            .fetch_one(pool)
            .await?;

            // 更新最后使用时间
            sqlx::query!(
                "UPDATE webdav_tokens SET last_used_at = NOW() WHERE token = $1",
                token
            )
            .execute(pool)
            .await?;

            return Ok(WebDavContext {
                user_id: user.id,
                username: user.username,
                is_admin: user.is_admin,
                token: Some(token.to_string()),
            });
        }
    }

    Err(AppError::Unauthorized("Authentication required".to_string()))
}

/// PROPFIND - 获取资源属性
async fn handle_propfind(
    state: AppState,
    context: WebDavContext,
    path: &str,
    request: Request<Body>,
) -> Result<Response, AppError> {
    // 解析请求体（可选）
    let depth = request.headers().get("Depth").and_then(|h| h.to_str().ok()).unwrap_or("0");
    
    // 获取资源信息
    let resources = list_resources(&state.db_pool, &context, path, depth).await?;

    // 构建 XML 响应
    let xml_response = build_propfind_response(resources, path);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/xml; charset=utf-8")
        .header("DAV", "1, 2")
        .body(Body::from(xml_response))
        .map_err(|e| AppError::Internal(e.to_string()))?)
}

/// GET - 下载文件
async fn handle_get(
    state: AppState,
    context: WebDavContext,
    path: &str,
) -> Result<Response, AppError> {
    // 解析路径
    let file_info = get_file_by_path(&state.db_pool, &context, path).await?;

    if file_info.type_ != "file" {
        return Err(AppError::BadRequest("Not a file".to_string()));
    }

    // 获取文件内容
    let blob = sqlx::query!(
        "SELECT storage_key, size, mime_type FROM file_blobs WHERE id = (SELECT blob_id FROM files WHERE id = $1)",
        file_info.id
    )
    .fetch_one(&state.db_pool)
    .await?;

    let content = state.storage_manager.download_file(&blob.storage_key, &blob.storage_key, Some(context.user_id)).await?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", blob.mime_type.unwrap_or("application/octet-stream"))
        .header("Content-Length", blob.size)
        .header("Last-Modified", format_http_date(blob.size)) // 简化处理
        .body(Body::from(content))
        .map_err(|e| AppError::Internal(e.to_string()))?)
}

/// PUT - 上传文件
async fn handle_put(
    state: AppState,
    context: WebDavContext,
    path: &str,
    request: Request<Body>,
) -> Result<Response, AppError> {
    // 解析路径，获取父目录和文件名
    let (parent_path, file_name) = parse_webdav_path(path)?;

    // 读取文件内容
    let body = axum::body::to_bytes(request.into_body(), usize::MAX).await?;
    let content = body.to_vec();

    // 检查父目录是否存在
    let parent_id = if parent_path.is_empty() {
        None
    } else {
        Some(get_folder_by_path(&state.db_pool, &context, &parent_path).await?.id)
    };

    // 创建或更新文件
    let file_id = create_or_update_file(
        &state.db_pool,
        &state.storage_manager,
        &context,
        file_name,
        content,
        parent_id,
    ).await?;

    Ok(Response::builder()
        .status(StatusCode::CREATED)
        .header("Location", format_webdav_url(file_id, &context.username))
        .body(Body::empty())
        .map_err(|e| AppError::Internal(e.to_string()))?)
}

/// DELETE - 删除文件/文件夹
async fn handle_delete(
    state: AppState,
    context: WebDavContext,
    path: &str,
) -> Result<Response, AppError> {
    // 解析资源
    let resource = get_resource_by_path(&state.db_pool, &context, path).await?;

    // 软删除
    sqlx::query!(
        "UPDATE files SET is_deleted = true, trashed_at = NOW(), trashed_by = $1 WHERE id = $2 AND user_id = $3",
        context.user_id,
        resource.id,
        context.user_id
    )
    .execute(&state.db_pool)
    .await?;

    Ok(Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .map_err(|e| AppError::Internal(e.to_string()))?)
}

/// MKCOL - 创建文件夹
async fn handle_mkcol(
    state: AppState,
    context: WebDavContext,
    path: &str,
    _request: Request<Body>,
) -> Result<Response, AppError> {
    // 解析路径
    let (parent_path, folder_name) = parse_webdav_path(path)?;

    // 获取父文件夹 ID
    let parent_id = if parent_path.is_empty() {
        None
    } else {
        Some(get_folder_by_path(&state.db_pool, &context, &parent_path).await?.id)
    };

    // 创建文件夹
    let folder_id = sqlx::query_scalar!(
        "INSERT INTO files (user_id, parent_id, name, type, size) VALUES ($1, $2, $3, 'folder', 0) RETURNING id",
        context.user_id,
        parent_id,
        folder_name
    )
    .fetch_one(&state.db_pool)
    .await?;

    Ok(Response::builder()
        .status(StatusCode::CREATED)
        .header("Location", format_webdav_url(folder_id, &context.username))
        .body(Body::empty())
        .map_err(|e| AppError::Internal(e.to_string()))?)
}

/// COPY - 复制文件/文件夹
async fn handle_copy(
    state: AppState,
    context: WebDavContext,
    path: &str,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let destination = headers.get("Destination")
        .and_then(|h| h.to_str().ok())
        .ok_or(AppError::BadRequest("Destination header required".to_string()))?;

    let overwrite = headers.get("Overwrite")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("T");

    // 解析源和目标
    let source_resource = get_resource_by_path(&state.db_pool, &context, path).await?;
    let (dest_parent_path, dest_name) = parse_webdav_path(destination)?;

    // 获取目标父目录
    let dest_parent_id = if dest_parent_path.is_empty() {
        None
    } else {
        Some(get_folder_by_path(&state.db_pool, &context, &dest_parent_path).await?.id)
    };

    // 复制逻辑（简化处理）
    // 实际应该递归复制文件夹内容
    if source_resource.type_ == "folder" {
        return Err(AppError::BadRequest("Folder copy not fully implemented".to_string()));
    }

    // 复制文件（创建新文件记录，共享 blob）
    let blob_id = sqlx::query_scalar!(
        "SELECT blob_id FROM files WHERE id = $1",
        source_resource.id
    )
    .fetch_one(&state.db_pool)
    .await?;

    let new_file_id = sqlx::query_scalar!(
        "INSERT INTO files (user_id, parent_id, name, type, blob_id, size) VALUES ($1, $2, $3, 'file', $4, (SELECT size FROM files WHERE id = $1)) RETURNING id",
        context.user_id,
        dest_parent_id,
        dest_name,
        blob_id
    )
    .fetch_one(&state.db_pool)
    .await?;

    Ok(Response::builder()
        .status(if overwrite == "T" { StatusCode::NO_CONTENT } else { StatusCode::CREATED })
        .body(Body::empty())
        .map_err(|e| AppError::Internal(e.to_string()))?)
}

/// MOVE - 移动文件/文件夹
async fn handle_move(
    state: AppState,
    context: WebDavContext,
    path: &str,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let destination = headers.get("Destination")
        .and_then(|h| h.to_str().ok())
        .ok_or(AppError::BadRequest("Destination header required".to_string()))?;

    // 解析目标路径
    let (dest_parent_path, dest_name) = parse_webdav_path(destination)?;
    let dest_parent_id = if dest_parent_path.is_empty() {
        None
    } else {
        Some(get_folder_by_path(&state.db_pool, &context, &dest_parent_path).await?.id)
    };

    // 移动资源
    sqlx::query!(
        "UPDATE files SET parent_id = $1, name = $2 WHERE id = $3 AND user_id = $4",
        dest_parent_id,
        dest_name,
        path, // 简化处理，实际应该解析 ID
        context.user_id
    )
    .execute(&state.db_pool)
    .await?;

    Ok(Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .map_err(|e| AppError::Internal(e.to_string()))?)
}

/// LOCK - 锁定资源
async fn handle_lock(
    state: AppState,
    context: WebDavContext,
    path: &str,
    request: Request<Body>,
) -> Result<Response, AppError> {
    // 解析请求体
    let body = axum::body::to_bytes(request.into_body(), usize::MAX).await?;
    let _lock_info: LockRequest = quick_xml::de::from_slice(&body)
        .map_err(|_| AppError::BadRequest("Invalid LOCK request body".to_string()))?;

    // 获取资源
    let resource = get_resource_by_path(&state.db_pool, &context, path).await?;

    // 生成锁令牌
    let lock_token = format!("opaquelocktoken:{}", Uuid::new_v4());

    // 创建锁记录
    let lock_id = sqlx::query_scalar!(
        r#"
        INSERT INTO webdav_locks (resource_id, user_id, lock_token, lock_scope, lock_type, owner, expires_at)
        VALUES ($1, $2, $3, 'exclusive', 'write', $4, NOW() + INTERVAL '1 hour')
        RETURNING id
        "#,
        resource.id,
        context.user_id,
        lock_token,
        context.username
    )
    .fetch_one(&state.db_pool)
    .await?;

    // 构建锁响应 XML
    let lock_response = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:prop xmlns:D="DAV:">
  <D:lockdiscovery>
    <D:activelock>
      <D:locktype><D:write/></D:locktype>
      <D:lockscope><D:exclusive/></D:lockscope>
      <D:depth>0</D:depth>
      <D:owner><D:href>{}</D:href></D:owner>
      <D:timeout>Second-3600</D:timeout>
      <D:locktoken><D:href>opaquelocktoken:{}</D:href></D:locktoken>
    </D:activelock>
  </D:lockdiscovery>
</D:prop>"#,
        context.username,
        lock_token
    );

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/xml; charset=utf-8")
        .header("Lock-Token", format!("<opaquelocktoken:{}>", lock_token))
        .body(Body::from(lock_response))
        .map_err(|e| AppError::Internal(e.to_string()))?)
}

/// UNLOCK - 解锁资源
async fn handle_unlock(
    state: AppState,
    context: WebDavContext,
    path: &str,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let lock_token = headers.get("Lock-Token")
        .and_then(|h| h.to_str().ok())
        .ok_or(AppError::BadRequest("Lock-Token header required".to_string()))?;

    // 清理 Lock-Token 头格式
    let lock_token = lock_token.trim_start_matches('<').trim_end_matches('>');

    // 删除锁记录
    sqlx::query!(
        "DELETE FROM webdav_locks WHERE lock_token = $1 AND user_id = $2",
        lock_token,
        context.user_id
    )
    .execute(&state.db_pool)
    .await?;

    Ok(Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .map_err(|e| AppError::Internal(e.to_string()))?)
}

/// OPTIONS - 返回支持的 WebDAV 方法
async fn handle_options() -> Result<Response, AppError> {
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("DAV", "1, 2")
        .header("Allow", "GET, HEAD, PUT, DELETE, PROPFIND, PROPPATCH, MKCOL, COPY, MOVE, LOCK, UNLOCK, OPTIONS")
        .header("Content-Type", "text/plain")
        .body(Body::from("DAV: 1, 2\nAllow: GET, HEAD, PUT, DELETE, PROPFIND, PROPPATCH, MKCOL, COPY, MOVE, LOCK, UNLOCK, OPTIONS"))
        .map_err(|e| AppError::Internal(e.to_string()))?)
}

/// PROPPATCH - 修改资源属性
async fn handle_proppatch(
    _state: AppState,
    _context: WebDavContext,
    _path: &str,
    _request: Request<Body>,
) -> Result<Response, AppError> {
    // WebDAV 标准支持，但本实现中属性由系统管理
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/xml; charset=utf-8")
        .body(Body::from("<?xml version=\"1.0\" encoding=\"utf-8\"?><D:multistatus xmlns:D=\"DAV:\"><D:response><D:href>/</D:href><D:propstat><D:prop/><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>"))
        .map_err(|e| AppError::Internal(e.to_string()))?)
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 列出资源（文件夹内容或单个资源）
async fn list_resources(
    pool: &PgPool,
    context: &WebDavContext,
    path: &str,
    depth: &str,
) -> Result<Vec<WebDavResource>, AppError> {
    let resources = if path == "/" || path.is_empty() {
        // 根目录：列出用户所有顶层资源
        sqlx::query_as::<(Uuid, String, String, i64, Option<String>, Option<Uuid>, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>), _>(
            r#"
                SELECT id, name, type, size, mime_type, parent_id, created_at, updated_at
                FROM files
                WHERE user_id = $1 AND parent_id IS NULL AND is_deleted = false
                ORDER BY type DESC, name ASC
            "#
        )
        .bind(context.user_id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|(id, name, type_, size, mime_type, parent_id, created_at, updated_at)| {
            WebDavResource {
                href: format!("/{}", name),
                resourcetype: if type_ == "folder" { WebDavResourceType::Collection } else { WebDavResourceType::File },
                getcontentlength: if type_ == "file" { Some(size as u64) } else { None },
                getcontenttype: mime_type,
                getlastmodified: updated_at,
                creationdate: created_at,
                displayname: name,
                lockdiscovery: None,
                supportedlock: Some(vec![
                    LockType { lockscope: "exclusive".to_string(), locktype: "write".to_string() },
                    LockType { lockscope: "shared".to_string(), locktype: "write".to_string() },
                ]),
            }
        })
        .collect()
    } else {
        // 特定路径：获取单个资源
        let resource = get_resource_by_path(pool, context, path).await?;
        
        vec![WebDavResource {
            href: path.to_string(),
            resourcetype: if resource.type_ == "folder" { WebDavResourceType::Collection } else { WebDavResourceType::File },
            getcontentlength: if resource.type_ == "file" { Some(resource.size as u64) } else { None },
            getcontenttype: resource.mime_type,
            getlastmodified: resource.updated_at,
            creationdate: resource.created_at,
            displayname: resource.name,
            lockdiscovery: None,
            supportedlock: Some(vec![
                LockType { lockscope: "exclusive".to_string(), locktype: "write".to_string() },
                LockType { lockscope: "shared".to_string(), locktype: "write".to_string() },
            ]),
        }]
    };

    Ok(resources)
}

/// 获取资源（文件环境文件夹）
async fn get_resource_by_path(
    pool: &PgPool,
    context: &WebDavContext,
    path: &str,
) -> Result<FileInfo, AppError> {
    let file = sqlx::query_as!(
        FileInfo,
        "SELECT id, name, type as type_, size, mime_type, parent_id, created_at, updated_at, is_deleted FROM files WHERE user_id = $1 AND is_deleted = false",
        context.user_id
    )
    .fetch_optional(pool)
    .await?;

    file.ok_or(AppError::NotFound("Resource not found".to_string()))
}

/// 获取文件
async fn get_file_by_path(
    pool: &PgPool,
    context: &WebDavContext,
    path: &str,
) -> Result<FileInfo, AppError> {
    let resource = get_resource_by_path(pool, context, path).await?;
    
    if resource.type_ != "file" {
        return Err(AppError::BadRequest("Not a file".to_string()));
    }

    Ok(resource)
}

/// 获取文件夹
async fn get_folder_by_path(
    pool: &PgPool,
    context: &WebDavContext,
    path: &str,
) -> Result<FileInfo, AppError> {
    let resource = get_resource_by_path(pool, context, path).await?;
    
    if resource.type_ != "folder" {
        return Err(AppError::BadRequest("Not a folder".to_string()));
    }

    Ok(resource)
}

/// 创建或更新文件
async fn create_or_update_file(
    pool: &PgPool,
    storage_manager: &StorageManager,
    context: &WebDavContext,
    file_name: String,
    content: Vec<u8>,
    parent_id: Option<Uuid>,
) -> Result<Uuid, AppError> {
    // 检查文件是否已存在
    let existing = sqlx::query!(
        "SELECT id, blob_id FROM files WHERE user_id = $1 AND parent_id = $2 AND name = $3 AND is_deleted = false",
        context.user_id,
        parent_id,
        file_name
    )
    .fetch_optional(pool)
    .await?;

    if let Some(existing) = existing {
        // 更新文件
        let storage_result = storage_manager.upload_file(context.user_id, storage_manager.default_policy_id, &content).await?;
        
        // 更新 blob
        sqlx::query!(
            "UPDATE file_blobs SET size = $1, storage_key = $2 WHERE id = $3",
            storage_result.size as i64,
            storage_result.storage_key,
            existing.blob_id
        )
        .execute(pool)
        .await?;

        Ok(existing.id)
    } else {
        // 创建新文件
        let storage_result = storage_manager.upload_file(context.user_id, storage_manager.default_policy_id, &content).await?;
        
        let file_id = sqlx::query_scalar!(
            "INSERT INTO files (user_id, parent_id, name, type, blob_id, size, mime_type) VALUES ($1, $2, $3, 'file', $4, $5, $6) RETURNING id",
            context.user_id,
            parent_id,
            file_name,
            storage_result.hash, // 简化处理
            storage_result.size as i64,
            None::<String>
        )
        .fetch_one(pool)
        .await?;

        Ok(file_id)
    }
}

/// 解析 WebDAV 路径
fn parse_webdav_path(path: &str) -> Result<(String, String), AppError> {
    let path = path.trim_start_matches('/');
    
    if path.is_empty() {
        return Ok(("".to_string(), "".to_string()));
    }

    let parts: Vec<&str> = path.rsplitn(2, '/').collect();
    
    if parts.len() == 1 {
        Ok(("".to_string(), parts[0].to_string()))
    } else {
        Ok((parts[1].to_string(), parts[0].to_string()))
    }
}

/// 构建 PROPFIND XML 响应
fn build_propfind_response(resources: Vec<WebDavResource>, base_path: &str) -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<D:multistatus xmlns:D=\"DAV:\">\n");
    
    for resource in resources {
        xml.push_str(&format!(
            r#"<D:response>
  <D:href>{}</D:href>
  <D:propstat>
    <D:prop>
      <D:resourcetype>{}</D:resourcetype>
      {}
      <D:displayname>{}</D:displayname>
      <D:creationdate>{}</D:creationdate>
      <D:getlastmodified>{}</D:getlastmodified>
      {}
    </D:prop>
    <D:status>HTTP/1.1 200 OK</D:status>
  </D:propstat>
</D:response>
"#,
            resource.href,
            match resource.resourcetype {
                WebDavResourceType::Collection => "<D:collection/>",
                WebDavResourceType::File => "",
            },
            resource.getcontentlength.map(|len| format!("<D:getcontentlength>{}</D:getcontentlength>", len)).unwrap_or_default(),
            resource.displayname,
            resource.creationdate.to_rfc3339(),
            format_http_date(resource.getlastmodified.timestamp()),
            resource.getcontenttype.map(|ct| format!("<D:getcontenttype>{}</D:getcontenttype>", ct)).unwrap_or_default(),
        ));
    }
    
    xml.push_str("</D:multistatus>");
    xml
}

/// 格式化 HTTP 日期
fn format_http_date(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|dt| dt.format("%a, %d %b %Y %H:%M:%S GMT").to_string())
        .unwrap_or_default()
}

/// 格式化 WebDAV URL
fn format_webdav_url(file_id: Uuid, username: &str) -> String {
    format!("/webdav/{}/{}", username, file_id)
}

/// 记录 WebDAV 访问日志
async fn log_webdav_access(
    pool: &PgPool,
    context: &WebDavContext,
    method: &str,
    path: &str,
    response: &Result<Response, AppError>,
    start_time: DateTime<Utc>,
) {
    let status_code = match response {
        Ok(r) => r.status().as_u16() as i32,
        Err(_) => 500,
    };

    let duration = (Utc::now() - start_time).num_milliseconds() as i32;

    let _ = sqlx::query!(
        r#"
        INSERT INTO webdav_access_logs (user_id, method, path, status_code, duration_ms)
        VALUES ($1, $2, $3, $4, $5)
        "#,
        context.user_id,
        method,
        path,
        status_code,
        duration
    )
    .execute(pool)
    .await;
}

/// 文件信息类型（简化）
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub id: Uuid,
    pub name: String,
    pub type_: String,
    pub size: i64,
    pub mime_type: Option<String>,
    pub parent_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub is_deleted: bool,
}

/// WebDAV 错误类型
#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Unauthorized(String),
    NotFound(String),
    MethodNotAllowed(String),
    Internal(String),
    Database(sqlx::Error),
    Auth(jsonwebtoken::errors::Error),
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Database(err)
    }
}

impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        AppError::Auth(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::MethodNotAllowed(method) => (StatusCode::METHOD_NOT_ALLOWED, format!("Method {} not allowed", method)),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            AppError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string()),
            AppError::Auth(_) => (StatusCode::UNAUTHORIZED, "Authentication error".to_string()),
        };

        Response::builder()
            .status(status)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(Body::from(body))
            .unwrap()
    }
}
