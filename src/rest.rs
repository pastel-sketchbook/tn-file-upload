//! REST API shim for browser SPA access.
//!
//! Provides HTTP endpoints that wrap the storage backend directly,
//! bypassing gRPC for browser clients that cannot use client-streaming.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{delete, get, post};
use bytes::Bytes;
use serde::Serialize;
use tokio_stream::StreamExt;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::storage::{Storage, StorageError};

/// Shared state for REST handlers.
pub struct RestState {
    pub storage: Arc<dyn Storage>,
    pub chunk_size: usize,
}

/// Build the axum router for REST endpoints.
pub fn router(state: Arc<RestState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let api = Router::new()
        .route("/upload", post(upload))
        .route("/files", get(list_files))
        .route("/files/{file_id}", get(get_metadata))
        .route("/files/{file_id}", delete(delete_file))
        .route("/files/{file_id}/download", get(download));

    Router::new()
        .nest("/api", api)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

// --- Response types ---

#[derive(Serialize)]
struct UploadResponse {
    file_id: String,
    size_bytes: u64,
    checksum: String,
}

#[derive(Serialize)]
struct FileMetaResponse {
    file_id: String,
    file_name: String,
    content_type: String,
    size_bytes: u64,
    sha256_checksum: String,
    uploaded_at: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

// --- Handlers ---

#[tracing::instrument(skip(state, multipart))]
async fn upload(
    State(state): State<Arc<RestState>>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, AppError> {
    let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::bad_request(e.to_string()))?
    else {
        return Err(AppError::bad_request("no file field".into()));
    };

    let file_name = field.file_name().unwrap_or("unnamed").to_owned();
    let content_type = field
        .content_type()
        .unwrap_or("application/octet-stream")
        .to_owned();

    let file_id = state.storage.create(&file_name, &content_type).await?;

    let data = field
        .bytes()
        .await
        .map_err(|e| AppError::bad_request(e.to_string()))?;

    // Write in chunk_size increments
    let mut offset = 0;
    while offset < data.len() {
        let end = (offset + state.chunk_size).min(data.len());
        let chunk = Bytes::copy_from_slice(&data[offset..end]);
        if let Err(e) = state.storage.append(&file_id, chunk).await {
            let _ = state.storage.abort(&file_id).await;
            return Err(e.into());
        }
        offset = end;
    }

    let meta = match state.storage.finalize(&file_id).await {
        Ok(m) => m,
        Err(e) => {
            let _ = state.storage.abort(&file_id).await;
            return Err(e.into());
        }
    };

    tracing::info!(file_id = %meta.file_id, size = meta.size_bytes, "upload complete");

    Ok(Json(UploadResponse {
        file_id: meta.file_id,
        size_bytes: meta.size_bytes,
        checksum: meta.sha256_checksum,
    }))
}

#[tracing::instrument(skip(state))]
async fn list_files(
    State(state): State<Arc<RestState>>,
) -> Result<Json<Vec<FileMetaResponse>>, AppError> {
    // The storage trait doesn't have a list method yet.
    // For now, return empty. We'll add list to the trait next.
    let _ = state;
    Ok(Json(vec![]))
}

#[tracing::instrument(skip(state))]
async fn get_metadata(
    State(state): State<Arc<RestState>>,
    Path(file_id): Path<String>,
) -> Result<Json<FileMetaResponse>, AppError> {
    let meta = state.storage.metadata(&file_id).await?;
    Ok(Json(FileMetaResponse {
        file_id: meta.file_id,
        file_name: meta.file_name,
        content_type: meta.content_type,
        size_bytes: meta.size_bytes,
        sha256_checksum: meta.sha256_checksum,
        uploaded_at: meta.uploaded_at,
    }))
}

#[tracing::instrument(skip(state))]
async fn delete_file(
    State(state): State<Arc<RestState>>,
    Path(file_id): Path<String>,
) -> Result<StatusCode, AppError> {
    state.storage.delete(&file_id).await?;
    tracing::info!(file_id = %file_id, "file deleted");
    Ok(StatusCode::NO_CONTENT)
}

#[tracing::instrument(skip(state))]
async fn download(
    State(state): State<Arc<RestState>>,
    Path(file_id): Path<String>,
) -> Result<Response, AppError> {
    let meta = state.storage.metadata(&file_id).await?;
    let mut stream = state
        .storage
        .read_chunks(&file_id, state.chunk_size)
        .await?;

    #[allow(clippy::cast_possible_truncation)]
    let mut body = Vec::with_capacity(meta.size_bytes as usize);
    while let Some(chunk) = stream.next().await {
        body.extend_from_slice(&chunk?);
    }

    Ok((
        StatusCode::OK,
        [
            ("content-type", meta.content_type),
            ("content-length", meta.size_bytes.to_string()),
            (
                "content-disposition",
                format!("attachment; filename=\"{}\"", meta.file_name),
            ),
        ],
        body,
    )
        .into_response())
}

// --- Error handling ---

struct AppError {
    status: StatusCode,
    message: String,
}

impl AppError {
    fn bad_request(msg: String) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg,
        }
    }
}

impl From<StorageError> for AppError {
    fn from(e: StorageError) -> Self {
        let status = match &e {
            StorageError::NotFound(_) => StatusCode::NOT_FOUND,
            StorageError::TooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            StorageError::InvalidFileName(_) => StatusCode::BAD_REQUEST,
            StorageError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        // Log internal errors server-side; return generic message to client
        if matches!(e, StorageError::Io(_)) {
            tracing::error!(error = %e, "internal storage error");
        }
        Self {
            status,
            message: match &e {
                StorageError::Io(_) => "internal server error".into(),
                _ => e.to_string(),
            },
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response()
    }
}
