pub mod local;

use async_trait::async_trait;
use bytes::Bytes;
use thiserror::Error;
use tokio_stream::Stream;

use crate::pb::FileMetadata;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("file too large: {size} exceeds limit {limit}")]
    TooLarge { size: u64, limit: u64 },
    #[error("invalid file name: {0}")]
    InvalidFileName(String),
}

/// Abstraction over file storage backends.
///
/// Uses `#[async_trait]` because the service holds `Arc<dyn Storage>` —
/// native async fn in traits is not yet dyn-compatible.
#[async_trait]
pub trait Storage: Send + Sync + 'static {
    /// Begin an upload, returning a file ID.
    async fn create(&self, file_name: &str, content_type: &str) -> Result<String, StorageError>;

    /// Append a chunk to an in-progress upload.
    async fn append(&self, file_id: &str, data: Bytes) -> Result<(), StorageError>;

    /// Finalize the upload, returning the completed metadata.
    async fn finalize(&self, file_id: &str) -> Result<FileMetadata, StorageError>;

    /// Abort an in-progress upload, cleaning up partial data.
    async fn abort(&self, file_id: &str) -> Result<(), StorageError>;

    /// Retrieve file metadata.
    async fn metadata(&self, file_id: &str) -> Result<FileMetadata, StorageError>;

    /// Stream file contents as chunks.
    async fn read_chunks(
        &self,
        file_id: &str,
        chunk_size: usize,
    ) -> Result<Box<dyn Stream<Item = Result<Bytes, StorageError>> + Send + Unpin>, StorageError>;

    /// Delete a file and its metadata.
    async fn delete(&self, file_id: &str) -> Result<(), StorageError>;

    /// List all finalized files.
    async fn list(&self) -> Result<Vec<FileMetadata>, StorageError>;
}
