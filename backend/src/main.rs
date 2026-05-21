mod auth;
mod config;
mod db;
mod file;
mod offline_download;
mod onedrive_auth;

mod preview;
mod share;
mod storage;
mod user;
mod webdav;

use axum::{
    body::Body,
    routing::{get, post, put, delete, head, patch, options},
    Json, Router,
};
use http::Method;
use serde::Serialize;
use std::path::PathBuf;
use tower_http::{
    cors::{CorsLayer, AllowOrigin, AllowMethods, AllowHeaders},
    trace::TraceLayer,
    services::ServeDir,
    limit::BodyLimitLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "notion_drive=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 加载配置
    let config = config::Config::init().expect("Failed to load configuration");

    // 初始化数据库连接池
    let db_pool = db::init_pool(&config.database.url)
        .await
        .expect("Failed to initialize database pool");

    // 运行数据库迁移
    db::run_migrations(&db_pool).await.expect("Failed to run migrations");

    // 初始化存储管理器
    let storage_manager = storage::StorageManager::new(&config.storage, &config.minio, db_pool.clone())
        .await
        .expect("Failed to initialize storage manager");

    // 构建 API 路由
    let api_routes = api_routes();

    // 构建前端静态文件服务
    let frontend_dir = PathBuf::from("/app/frontend");
    let frontend_service = if frontend_dir.exists() {
        tracing::info!("📁 Serving frontend from /app/frontend");
        ServeDir::new(&frontend_dir)
            .precompressed_gzip()
            .precompressed_br()
    } else {
        tracing::warn!("⚠️ Frontend directory not found at /app/frontend, serving placeholder");
        ServeDir::new("/app/frontend")
    };

    // 构建应用路由
    // 路由优先级：API > WebDAV > 前端静态文件
    let app = Router::new()
        // 健康检查
        .route("/health", get(health_check))
        // API 路由
        .nest("/api/v1", api_routes)
        // WebDAV 路由
        .nest("/webdav", webdav_routes())
        // 前端静态文件（所有其他请求）
        .fallback_service(frontend_service)
        // 注入状态
        .with_state(AppState {
            config: config.clone(),
            db_pool,
            storage_manager,
        })
        // CORS 中间件 - 使用配置中的白名单
        .layer(build_cors_layer(&config.cors))
        // 请求体大小限制 - 防止内存耗尽攻击（最大 100MB）
        .layer(BodyLimitLayer::new(100 * 1024 * 1024))
        .layer(TraceLayer::new_for_http());

    // 启动服务器
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("Failed to bind to port 8080");
    
    tracing::info!("🚀 Server starting on http://0.0.0.0:8080");
    tracing::info!("📱 Frontend: http://localhost:8080");
    tracing::info!("🔌 API: http://localhost:8080/api/v1");
    tracing::info!("📁 WebDAV: http://localhost:8080/webdav");
    
    axum::serve(listener, app).await.unwrap();
}

#[derive(Serialize)]
struct HealthStatus {
    status: String,
    database: String,
    storage: String,
    timestamp: u64,
}

async fn health_check(State(state): State<AppState>) -> Json<HealthStatus> {
    // 检查数据库连接
    let db_status = sqlx::query!("SELECT 1")
        .execute(&state.db_pool)
        .await
        .map(|_| "healthy")
        .unwrap_or("unhealthy");

    // 检查存储（简化检查）
    let storage_status = "healthy"; // 实际应用中应检查存储后端

    Json(HealthStatus {
        status: if db_status == "healthy" && storage_status == "healthy" { "ok" } else { "degraded" },
        database: db_status.to_string(),
        storage: storage_status.to_string(),
        timestamp: chrono::Utc::now().timestamp() as u64,
    })
}

#[derive(Clone)]
struct AppState {
    config: config::Config,
    db_pool: sqlx::PgPool,
    storage_manager: storage::StorageManager,
}

fn api_routes() -> Router<AppState> {
    Router::new()
        // 认证路由
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/refresh", post(auth::refresh_token))
        .route("/auth/me", get(auth::get_current_user))
        
        // 用户路由
        .route("/users/profile", get(user::get_profile))
        .route("/users/profile", put(user::update_profile))
        .route("/users/quota", get(user::get_quota))
        .route("/users/recycle-bin", get(user::get_recycle_bin))
        .route("/users/recycle-bin/:file_id/restore", put(user::restore_from_recycle_bin))
        .route("/users/recycle-bin/:file_id/delete", delete(user::permanent_delete))
        .route("/users/webdav-tokens", get(user::list_webdav_tokens))
        .route("/users/webdav-tokens", post(user::create_webdav_token))
        .route("/users/webdav-tokens/:token_id", delete(user::delete_webdav_token))
        .route("/users/webdav-usage", get(user::get_webdav_usage))
        
        // 文件路由
        .route("/files", get(file::list_files))
        .route("/files", post(file::create_folder))
        .route("/files/upload/simple", post(file::upload_file_simple))
        .route("/files/upload/session", post(file::create_upload_session))
        .route("/files/upload/chunk", put(file::upload_chunk))
        .route("/files/upload/session/:session_id/complete", put(file::complete_upload_session))
        .route("/files/upload/session/:session_id/cancel", delete(file::cancel_upload_session))
        .route("/files/:file_id", get(file::get_file_info))
        .route("/files/:file_id", delete(file::delete_file))
        .route("/files/:file_id/move", put(file::move_file))
        .route("/files/:file_id/rename", put(file::rename_file))
        .route("/files/:file_id/download", get(file::download_file))
        .route("/files/:file_id/versions", get(file::list_file_versions))
        .route("/files/search", get(file::search_files))
        
        // 预览路由
        .route("/preview/:file_id", get(preview::get_file_preview))
        .route("/preview/office/:file_id", get(preview::get_office_preview))
        
        // 分享路由
        .route("/shares", get(share::list_shares))
        .route("/shares", post(share::create_share))
        .route("/shares/:share_id", delete(share::delete_share))
        .route("/shares/public/:token", get(share::get_share_info))
        .route("/shares/public/:token/download", get(share::download_share))
        
        // 离线下载路由
        .route("/offline-download", post(offline_download::create_offline_download))
        .route("/offline-download", get(offline_download::list_offline_downloads))
        .route("/offline-download/:task_id", delete(offline_download::cancel_offline_download))
        
        // OneDrive 授权路由
        .route("/storage/onedrive/authorize", get(onedrive_auth::get_onedrive_authorization_url))
        .route("/storage/onedrive/callback/:policy_id", get(onedrive_auth::onedrive_callback))
        .route("/storage/onedrive/status/:policy_id", get(onedrive_auth::get_onedrive_token_status))
        .route("/storage/onedrive/disconnect/:policy_id", delete(onedrive_auth::disconnect_onedrive))
        
        // 团队协作路由
        .route("/teams", get(user::list_teams))
        .route("/teams", post(user::create_team))
        .route("/teams/:team_id", delete(user::delete_team))
        .route("/teams/:team_id/members", get(user::list_team_members))
        .route("/teams/:team_id/members", post(user::invite_team_member))
        .route("/teams/:team_id/members/:user_id", delete(user::remove_team_member))
        .route("/teams/:team_id/files/:file_id/permissions", put(user::set_file_permission))
}

fn webdav_routes() -> Router<AppState> {
    Router::new()
        // WebDAV 根路径
        .route("/", get(webdav::webdav_handler))
        .route("/", head(webdav::webdav_handler))
        .route("/", post(webdav::webdav_handler))
        .route("/", put(webdav::webdav_handler))
        .route("/", delete(webdav::webdav_handler))
        .route("/", patch(webdav::webdav_handler))
        .route("/", options(webdav::webdav_handler))
        // 用户 WebDAV 路径
        .route("/:username", get(webdav::webdav_handler))
        .route("/:username", head(webdav::webdav_handler))
        .route("/:username", post(webdav::webdav_handler))
        .route("/:username", put(webdav::webdav_handler))
        .route("/:username", delete(webdav::webdav_handler))
        .route("/:username", patch(webdav::webdav_handler))
        .route("/:username", options(webdav::webdav_handler))
        // 文件/文件夹路径
        .route("/:username/*path", get(webdav::webdav_handler))
        .route("/:username/*path", head(webdav::webdav_handler))
        .route("/:username/*path", post(webdav::webdav_handler))
        .route("/:username/*path", put(webdav::webdav_handler))
        .route("/:username/*path", delete(webdav::webdav_handler))
        .route("/:username/*path", patch(webdav::webdav_handler))
        .route("/:username/*path", options(webdav::webdav_handler))
}

/// 构建 CORS 层 - 使用配置中的白名单
fn build_cors_layer(cors_config: &config::CorsConfig) -> CorsLayer {
    let mut layer = CorsLayer::new();

    // 允许的来源
    for origin in &cors_config.allowed_origins {
        if let Ok(parsed) = origin.parse() {
            layer = layer.allow_origin(parsed);
        }
    }

    // 允许的方法
    let mut methods = AllowMethods::none();
    for method_str in &cors_config.allowed_methods {
        if let Ok(method) = Method::from_bytes(method_str.as_bytes()) {
            methods = methods.allow(method);
        }
    }
    layer = layer.allow_methods(methods);

    // 允许的头部
    let mut headers = AllowHeaders::none();
    for header in &cors_config.allowed_headers {
        if let Ok(parsed) = header.parse() {
            headers = headers.allow(parsed);
        }
    }
    layer = layer.allow_headers(headers);

    layer
}
