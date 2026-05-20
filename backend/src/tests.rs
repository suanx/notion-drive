#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use sqlx::PgPool;
    use tower::ServiceExt;

    fn create_test_app(pool: PgPool) -> Router {
        let config = config::Config::init().unwrap();
        let storage_manager = storage::StorageManager::new(&config.storage, &config.minio, pool.clone())
            .await
            .unwrap();

        Router::new()
            .route("/health", get(health_check))
            .nest("/api/v1", api_routes())
            .with_state(AppState {
                config,
                db_pool: pool,
                storage_manager,
            })
            .layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            )
    }

    #[tokio::test]
    async fn test_health_check() {
        let pool = create_test_pool().await;
        let app = create_test_app(pool).await;

        let response = app
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_register_user() {
        let pool = create_test_pool().await;
        let app = create_test_app(pool).await;

        let payload = serde_json::json!({
            "username": "testuser",
            "email": "test@example.com",
            "password": "password123"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/auth/register")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_register_duplicate_email() {
        let pool = create_test_pool().await;
        let app = create_test_app(pool).await;

        // 先注册一个用户
        let payload1 = serde_json::json!({
            "username": "testuser1",
            "email": "duplicate@example.com",
            "password": "password123"
        });

        app.clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/auth/register")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&payload1).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // 尝试用相同邮箱注册
        let payload2 = serde_json::json!({
            "username": "testuser2",
            "email": "duplicate@example.com",
            "password": "password123"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/auth/register")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&payload2).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_login_success() {
        let pool = create_test_pool().await;
        let app = create_test_app(pool).await;

        // 先注册
        let register_payload = serde_json::json!({
            "username": "loginuser",
            "email": "login@example.com",
            "password": "password123"
        });

        app.clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/auth/register")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&register_payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // 登录
        let login_payload = serde_json::json!({
            "email": "login@example.com",
            "password": "password123"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/auth/login")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&login_payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_login_invalid_credentials() {
        let pool = create_test_pool().await;
        let app = create_test_app(pool).await;

        let payload = serde_json::json!({
            "email": "nonexistent@example.com",
            "password": "wrongpassword"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/auth/login")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_create_folder() {
        let pool = create_test_pool().await;
        let app = create_test_app(pool).await;

        // 注册并登录获取 token
        let (token, user_id) = register_and_login(&app).await;

        // 创建文件夹
        let payload = serde_json::json!({
            "name": "Test Folder",
            "parent_id": null
        });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/files")
                    .header("Content-Type", "application/json")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_delete_file_soft() {
        let pool = create_test_pool().await;
        let app = create_test_app(pool).await;

        let (token, user_id) = register_and_login(&app).await;

        // 创建文件夹
        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/files")
                    .header("Content-Type", "application/json")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::from(serde_json::to_string(&serde_json::json!({
                        "name": "Delete Test",
                        "parent_id": null
                    })).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let create_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(create_response.into_body(), usize::MAX).await.unwrap()
        ).unwrap();
        let file_id = create_body["id"].as_str().unwrap();

        // 软删除
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/files/{}", file_id))
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // 验证文件已被软删除
        let list_response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/files")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let list_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(list_response.into_body(), usize::MAX).await.unwrap()
        ).unwrap();
        
        let files = list_body["files"].as_array().unwrap();
        assert!(!files.iter().any(|f| f["id"].as_str() == Some(file_id)));
    }

    async fn create_test_pool() -> PgPool {
        // 使用测试数据库
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://test:test@localhost:5432/notion_drive_test".to_string());
        
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to create test pool")
    }

    async fn register_and_login(app: &Router) -> (String, Uuid) {
        // 注册
        let register_payload = serde_json::json!({
            "username": "testuser",
            "email": "testauth@example.com",
            "password": "password123"
        });

        let register_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/auth/register")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&register_payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let register_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(register_response.into_body(), usize::MAX).await.unwrap()
        ).unwrap();
        let token = register_body["token"].as_str().unwrap().to_string();
        let user_id = Uuid::parse_str(register_body["user"]["id"].as_str().unwrap()).unwrap();

        (token, user_id)
    }
}
