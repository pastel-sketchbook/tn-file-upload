//! REST API shim for browser SPA access.
//!
//! Provides HTTP endpoints that wrap the storage backend directly,
//! bypassing gRPC for browser clients that cannot use client-streaming.

use std::sync::Arc;

use axum::Router;
use axum::extract::{DefaultBodyLimit, Multipart, Path, Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{delete, get, post};
use serde::Serialize;
use subtle::ConstantTimeEq;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::storage::{Storage, StorageError};

/// Shared state for REST handlers.
pub struct RestState {
    pub storage: Arc<dyn Storage>,
    pub chunk_size: usize,
    pub max_file_size: u64,
    pub auth_token: String,
}

/// Build the axum router for REST endpoints.
pub fn router(state: Arc<RestState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Allow uploads up to max_file_size + overhead for multipart framing
    #[allow(clippy::cast_possible_truncation)]
    let body_limit = DefaultBodyLimit::max(state.max_file_size as usize + 1024 * 1024);

    let api = Router::new()
        .route("/upload", post(upload))
        .route("/files", get(list_files))
        .route("/files/{file_id}", get(get_metadata))
        .route("/files/{file_id}", delete(delete_file))
        .route("/files/{file_id}/download", get(download))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    Router::new()
        .nest("/api", api)
        .layer(body_limit)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Validates the `x-auth-token` header on every REST request.
async fn auth_middleware(
    State(state): State<Arc<RestState>>,
    req: Request,
    next: Next,
) -> Result<Response, Response> {
    let token = req
        .headers()
        .get("x-auth-token")
        .and_then(|v| v.to_str().ok());

    match token {
        Some(t) if t.as_bytes().ct_eq(state.auth_token.as_bytes()).into() => {
            Ok(next.run(req).await)
        }
        Some(_) => Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "invalid auth token".into(),
            }),
        )
            .into_response()),
        None => Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "missing auth token".into(),
            }),
        )
            .into_response()),
    }
}

// --- Response types ---

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadResponse {
    file_id: String,
    size_bytes: u64,
    checksum: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
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
    let Some(mut field) = multipart
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

    // Stream chunks directly from the multipart field without buffering the whole file
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|e| AppError::bad_request(e.to_string()))?
    {
        if let Err(e) = state.storage.append(&file_id, chunk).await {
            let _ = state.storage.abort(&file_id).await;
            return Err(e.into());
        }
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
    let files = state.storage.list().await?;
    let response: Vec<FileMetaResponse> = files
        .into_iter()
        .map(|m| FileMetaResponse {
            file_id: m.file_id,
            file_name: m.file_name,
            content_type: m.content_type,
            size_bytes: m.size_bytes,
            sha256_checksum: m.sha256_checksum,
            uploaded_at: m.uploaded_at,
        })
        .collect();
    Ok(Json(response))
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
    let stream = state
        .storage
        .read_chunks(&file_id, state.chunk_size)
        .await?;

    // Stream the file directly without buffering in memory
    let body = axum::body::Body::from_stream(stream);

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

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    use super::*;
    use crate::storage::local::LocalStorage;

    async fn test_state(dir: &std::path::Path) -> Arc<RestState> {
        let storage = LocalStorage::new(dir.to_str().unwrap(), 10 * 1024 * 1024)
            .await
            .unwrap();
        Arc::new(RestState {
            storage: Arc::new(storage),
            chunk_size: 4096,
            max_file_size: 10 * 1024 * 1024,
            auth_token: "test-secret".into(),
        })
    }

    #[tokio::test]
    async fn rejects_missing_auth_token() {
        let dir = tempfile::tempdir().unwrap();
        let state = test_state(dir.path()).await;
        let app = router(state);

        let req = Request::builder()
            .uri("/api/files")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn rejects_invalid_auth_token() {
        let dir = tempfile::tempdir().unwrap();
        let state = test_state(dir.path()).await;
        let app = router(state);

        let req = Request::builder()
            .uri("/api/files")
            .header("x-auth-token", "wrong-token")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn accepts_valid_auth_token() {
        let dir = tempfile::tempdir().unwrap();
        let state = test_state(dir.path()).await;
        let app = router(state);

        let req = Request::builder()
            .uri("/api/files")
            .header("x-auth-token", "test-secret")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        // 200 OK (empty file list) — not 401
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
