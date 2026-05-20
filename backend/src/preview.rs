use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::AuthExtractor,
    AppState,
};

#[derive(Debug, Serialize)]
pub struct PreviewInfo {
    pub supported: bool,
    pub preview_type: Option<String>, // 'image', 'video', 'pdf', 'text', 'office'
    pub url: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OfficePreviewResponse {
    pub supported: bool,
    pub message: String,
}

pub async fn get_file_preview(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Path(file_id): Path<Uuid>,
) -> Result<Json<PreviewInfo>, AppError> {
    let claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    // 获取文件信息
    let file = sqlx::query!(
        "SELECT id, name, mime_type, blob_id FROM files WHERE id = $1 AND user_id = $2 AND is_deleted = false",
        file_id,
        claims.sub
    )
    .fetch_optional(&state.db_pool)
    .await?;

    let file = match file {
        Some(f) => f,
        None => return Err(AppError::NotFound("File not found".to_string())),
    };

    let mime_type = file.mime_type.as_deref().unwrap_or("application/octet-stream");
    
    // 判断文件类型
    let preview_info = match mime_type {
        "image/jpeg" | "image/png" | "image/gif" | "image/webp" | "image/svg+xml" => {
            // 图片预览 - 生成预签名 URL
            let blob = sqlx::query!(
                "SELECT storage_key FROM file_blobs WHERE id = $1",
                file.blob_id
            )
            .fetch_one(&state.db_pool)
            .await?;

            let url = state
                .storage_manager
                .generate_presigned_url(&blob.storage_key, 3600)
                .await
                .ok();

            PreviewInfo {
                supported: true,
                preview_type: Some("image".to_string()),
                url,
                message: None,
            }
        }
        "video/mp4" | "video/webm" | "video/ogg" => {
            // 视频预览
            let blob = sqlx::query!(
                "SELECT storage_key FROM file_blobs WHERE id = $1",
                file.blob_id
            )
            .fetch_one(&state.db_pool)
            .await?;

            let url = state
                .storage_manager
                .generate_presigned_url(&blob.storage_key, 3600)
                .await
                .ok();

            PreviewInfo {
                supported: true,
                preview_type: Some("video".to_string()),
                url,
                message: None,
            }
        }
        "application/pdf" => {
            // PDF 预览 - 需要后端转换或前端渲染
            PreviewInfo {
                supported: true,
                preview_type: Some("pdf".to_string()),
                url: None, // PDF 需要特殊处理
                message: Some("PDF 预览功能需要集成 PDF.js".to_string()),
            }
        }
        "text/plain" | "text/html" | "text/css" | "text/javascript" |
        "application/json" | "application/xml" => {
            // 文本文件预览
            let blob = sqlx::query!(
                "SELECT storage_key, size FROM file_blobs WHERE id = $1",
                file.blob_id
            )
            .fetch_one(&state.db_pool)
            .await?;

            // 检查文件大小（只预览小文件）
            if blob.size > 10 * 1024 * 1024 {
                return Ok(Json(PreviewInfo {
                    supported: false,
                    preview_type: None,
                    url: None,
                    message: Some("文件过大，不支持预览".to_string()),
                }));
            }

            let content = state.storage_manager.download_file(&blob.storage_key).await?;
            
            // 返回文本内容（限制长度）
            let content_str = String::from_utf8_lossy(&content);
            let preview_content = if content_str.len() > 10000 {
                format!("{}...\n[文件过大，仅显示前 10KB]", &content_str[..10000])
            } else {
                content_str.to_string()
            };

            // 这里应该返回一个临时的预览 URL
            // 简化处理：直接返回内容（实际应该使用临时存储）
            PreviewInfo {
                supported: true,
                preview_type: Some("text".to_string()),
                url: None,
                message: Some(format!("预览内容（{} bytes）", blob.size)),
            }
        }
        "application/msword" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document" |
        "application/vnd.ms-excel" | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" |
        "application/vnd.ms-powerpoint" | "application/vnd.openxmlformats-officedocument.presentationml.presentation" => {
            // Office 文档预览
            PreviewInfo {
                supported: false,
                preview_type: None,
                url: None,
                message: Some("Office 文档预览需要集成 LibreOffice 或 OnlyOffice".to_string()),
            }
        }
        _ => {
            PreviewInfo {
                supported: false,
                preview_type: None,
                url: None,
                message: Some("不支持预览的文件类型".to_string()),
            }
        }
    };

    Ok(Json(preview_info))
}

pub async fn get_office_preview(
    State(state): State<AppState>,
    auth: AuthExtractor,
    Path(file_id): Path<Uuid>,
) -> Result<Json<OfficePreviewResponse>, AppError> {
    let _claims = crate::auth::validate_jwt(&auth.token, &state.config.jwt.secret)?;

    // 这里应该集成 LibreOffice 或 OnlyOffice
    // 简化处理：返回不支持
    Ok(Json(OfficePreviewResponse {
        supported: false,
        message: "Office 文档预览功能正在开发中，需要集成 LibreOffice 或 OnlyOffice".to_string(),
    }))
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

impl From<crate::storage::StorageError> for AppError {
    fn from(err: crate::storage::StorageError) -> Self {
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
