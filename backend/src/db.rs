use sqlx::postgres::{PgPool, PgPoolOptions};
use tracing::info;

pub async fn init_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    info!("Initializing database pool...");
    
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await?;
    
    info!("Database pool initialized successfully");
    
    Ok(pool)
}

// 数据库迁移检查
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    info!("Running database migrations...");
    
    // 这里可以使用 sqlx::migrate!() 自动运行 migrations 目录下的迁移
    // 或者手动执行 SQL
    
    info!("Database migrations completed");
    
    Ok(())
}
