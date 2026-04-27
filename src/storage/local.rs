use std::path::{Path, PathBuf};

use async_trait::async_trait;
use bytes::Bytes;
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio_stream::Stream;
use uuid::Uuid;

use crate::pb::FileMetadata;

use super::{Storage, StorageError};

/// Local filesystem storage backend.
pub struct LocalStorage {
    base_path: PathBuf,
    max_file_size: u64,
}

impl LocalStorage {
    /// Create a new local storage backend, ensuring the base directory exists.
    ///
    /// # Errors
    ///
    /// Returns `StorageError::Io` if the directory cannot be created.
    #[tracing::instrument(skip(base_path))]
    pub async fn new(
        base_path: impl Into<PathBuf>,
        max_file_size: u64,
    ) -> Result<Self, StorageError> {
        let base_path = base_path.into();
        fs::create_dir_all(&base_path).await?;
        Ok(Self {
            base_path,
            max_file_size,
        })
    }

    fn data_path(&self, file_id: &str) -> PathBuf {
        self.base_path.join(format!("{file_id}.data"))
    }

    fn meta_path(&self, file_id: &str) -> PathBuf {
        self.base_path.join(format!("{file_id}.meta"))
    }

    /// Validate file name: no path traversal, no empty name.
    fn validate_file_name(name: &str) -> Result<(), StorageError> {
        if name.is_empty() {
            return Err(StorageError::InvalidFileName("empty file name".into()));
        }
        let path = Path::new(name);
        for component in path.components() {
            match component {
                std::path::Component::Normal(_) => {}
                _ => {
                    return Err(StorageError::InvalidFileName(format!(
                        "path traversal detected: {name}"
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Simple JSON metadata stored alongside data files.
#[derive(serde::Serialize, serde::Deserialize)]
struct MetaRecord {
    file_id: String,
    file_name: String,
    content_type: String,
    size_bytes: u64,
    sha256_checksum: String,
    uploaded_at: String,
    finalized: bool,
}

#[async_trait]
impl Storage for LocalStorage {
    async fn create(&self, file_name: &str, content_type: &str) -> Result<String, StorageError> {
        Self::validate_file_name(file_name)?;

        let file_id = Uuid::now_v7().to_string();

        // Create empty data file
        fs::File::create(self.data_path(&file_id)).await?;

        // Write initial metadata
        let meta = MetaRecord {
            file_id: file_id.clone(),
            file_name: file_name.to_owned(),
            content_type: content_type.to_owned(),
            size_bytes: 0,
            sha256_checksum: String::new(),
            uploaded_at: String::new(),
            finalized: false,
        };
        let json =
            serde_json::to_vec(&meta).map_err(|e| StorageError::Io(std::io::Error::other(e)))?;
        fs::write(self.meta_path(&file_id), json).await?;

        Ok(file_id)
    }

    async fn append(&self, file_id: &str, data: Bytes) -> Result<(), StorageError> {
        let meta_path = self.meta_path(file_id);
        if !meta_path.exists() {
            return Err(StorageError::NotFound(file_id.to_owned()));
        }

        // Check size limit
        let data_path = self.data_path(file_id);
        let current_size = fs::metadata(&data_path).await?.len();
        let new_size = current_size + data.len() as u64;
        if new_size > self.max_file_size {
            return Err(StorageError::TooLarge {
                size: new_size,
                limit: self.max_file_size,
            });
        }

        let mut file = fs::OpenOptions::new().append(true).open(&data_path).await?;
        file.write_all(&data).await?;
        file.flush().await?;

        Ok(())
    }

    async fn finalize(&self, file_id: &str) -> Result<FileMetadata, StorageError> {
        let meta_path = self.meta_path(file_id);
        let meta_bytes = fs::read(&meta_path)
            .await
            .map_err(|_| StorageError::NotFound(file_id.to_owned()))?;
        let mut meta: MetaRecord = serde_json::from_slice(&meta_bytes)
            .map_err(|e| StorageError::Io(std::io::Error::other(e)))?;

        // Compute checksum and size
        let data = fs::read(self.data_path(file_id)).await?;
        let checksum = hex::encode(Sha256::digest(&data));
        let size = data.len() as u64;
        let now = chrono::Utc::now().to_rfc3339();

        meta.size_bytes = size;
        meta.sha256_checksum = checksum;
        meta.uploaded_at = now;
        meta.finalized = true;

        let json =
            serde_json::to_vec(&meta).map_err(|e| StorageError::Io(std::io::Error::other(e)))?;
        fs::write(&meta_path, json).await?;

        Ok(FileMetadata {
            file_id: file_id.to_owned(),
            file_name: meta.file_name,
            content_type: meta.content_type,
            size_bytes: size,
            sha256_checksum: meta.sha256_checksum,
            uploaded_at: meta.uploaded_at,
        })
    }

    async fn abort(&self, file_id: &str) -> Result<(), StorageError> {
        let _ = fs::remove_file(self.data_path(file_id)).await;
        let _ = fs::remove_file(self.meta_path(file_id)).await;
        Ok(())
    }

    async fn metadata(&self, file_id: &str) -> Result<FileMetadata, StorageError> {
        let meta_bytes = fs::read(self.meta_path(file_id))
            .await
            .map_err(|_| StorageError::NotFound(file_id.to_owned()))?;
        let meta: MetaRecord = serde_json::from_slice(&meta_bytes)
            .map_err(|e| StorageError::Io(std::io::Error::other(e)))?;

        if !meta.finalized {
            return Err(StorageError::NotFound(file_id.to_owned()));
        }

        Ok(FileMetadata {
            file_id: meta.file_id,
            file_name: meta.file_name,
            content_type: meta.content_type,
            size_bytes: meta.size_bytes,
            sha256_checksum: meta.sha256_checksum,
            uploaded_at: meta.uploaded_at,
        })
    }

    async fn read_chunks(
        &self,
        file_id: &str,
        chunk_size: usize,
    ) -> Result<Box<dyn Stream<Item = Result<Bytes, StorageError>> + Send + Unpin>, StorageError>
    {
        // Verify file exists and is finalized
        self.metadata(file_id).await?;

        let data_path = self.data_path(file_id);
        let data = fs::read(&data_path).await?;

        let chunks: Vec<Result<Bytes, StorageError>> = data
            .chunks(chunk_size)
            .map(|c| Ok(Bytes::copy_from_slice(c)))
            .collect();

        Ok(Box::new(tokio_stream::iter(chunks)))
    }

    async fn delete(&self, file_id: &str) -> Result<(), StorageError> {
        // Verify exists
        if !self.meta_path(file_id).exists() {
            return Err(StorageError::NotFound(file_id.to_owned()));
        }
        let _ = fs::remove_file(self.data_path(file_id)).await;
        let _ = fs::remove_file(self.meta_path(file_id)).await;
        Ok(())
    }
}
