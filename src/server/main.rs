use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::net::TcpListener;
use tonic::transport::Server;
use tonic::{Request, Status};
use tracing_subscriber::EnvFilter;

use tn_file_upload::auth::auth_interceptor;
use tn_file_upload::config::Config;
use tn_file_upload::health::{AppState, health_service};
use tn_file_upload::interceptor::request_id_interceptor;
use tn_file_upload::pb::file_upload_server::FileUploadServer;
use tn_file_upload::rest::{self, RestState};
use tn_file_upload::service::FileUploadService;
use tn_file_upload::storage::local::LocalStorage;

fn combined_interceptor(req: Request<()>) -> Result<Request<()>, Status> {
    let req = request_id_interceptor(req)?;
    auth_interceptor(req)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(true)
        .json()
        .init();

    let config = Config::from_env().context("loading config")?;

    let storage = LocalStorage::new(&config.storage_path, config.max_file_size)
        .await
        .context("initializing storage")?;

    let storage = Arc::new(storage);
    let state = Arc::new(AppState::new());
    let service = FileUploadService::new(storage.clone(), config.chunk_size);

    // REST API server for browser SPA
    let rest_addr = config.rest_addr.as_deref().unwrap_or("[::]:3001");
    let rest_state = Arc::new(RestState {
        storage: storage.clone(),
        chunk_size: config.chunk_size,
        max_file_size: config.max_file_size,
    });
    let rest_router = rest::router(rest_state);
    let rest_listener = TcpListener::bind(rest_addr)
        .await
        .context("binding REST listener")?;
    tracing::info!(addr = %rest_addr, "REST API server listening");

    tokio::spawn(async move {
        axum::serve(rest_listener, rest_router).await.ok();
    });

    // gRPC server
    let listener = TcpListener::bind(&config.listen_addr)
        .await
        .context("binding listener")?;

    tracing::info!(addr = %config.listen_addr, "gRPC file upload server listening");

    Server::builder()
        .add_service(FileUploadServer::with_interceptor(
            service,
            combined_interceptor,
        ))
        .add_service(health_service(state))
        .serve_with_incoming_shutdown(
            tokio_stream::wrappers::TcpListenerStream::new(listener),
            async {
                tokio::signal::ctrl_c().await.ok();
                tracing::info!("shutdown signal received");
            },
        )
        .await
        .context("serving")?;

    Ok(())
}
