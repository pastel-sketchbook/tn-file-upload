use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio_util::sync::CancellationToken;
use tonic_health::pb::health_server::HealthServer;
use tonic_health::server::health_reporter;

use crate::pb::file_upload_server::FileUploadServer;
use crate::service::FileUploadService;

/// Shared application state for health monitoring.
pub struct AppState {
    pub healthy: AtomicBool,
}

impl AppState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            healthy: AtomicBool::new(true),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Create the gRPC health service and spawn a background monitor.
///
/// The monitor task exits when `cancel` is cancelled, preventing a
/// leaked task that runs for the lifetime of the process.
pub fn health_service(
    state: Arc<AppState>,
    cancel: CancellationToken,
) -> HealthServer<impl tonic_health::pb::health_server::Health> {
    let (reporter, health_service) = health_reporter();

    tokio::spawn(async move {
        reporter
            .set_serving::<FileUploadServer<FileUploadService>>()
            .await;

        loop {
            tokio::select! {
                () = cancel.cancelled() => break,
                () = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
            }

            if state.healthy.load(Ordering::Relaxed) {
                reporter
                    .set_serving::<FileUploadServer<FileUploadService>>()
                    .await;
            } else {
                reporter
                    .set_not_serving::<FileUploadServer<FileUploadService>>()
                    .await;
            }
        }
    });

    health_service
}
