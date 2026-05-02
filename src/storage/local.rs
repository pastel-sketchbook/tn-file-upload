use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;
use bytes::Bytes;
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_stream::Stream;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::pb::FileMetadata;

use super::{Storage, StorageError};

/// Local filesystem storage backend.
pub struct LocalStorage {
    base_path: PathBuf,
    max_file_size: u64,
    /// In-flight upload hashers, keyed by `file_id`.
    hashers: Mutex<HashMap<String, Sha256>>,
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
            hashers: Mutex::new(HashMap::new()),
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

        // Initialize streaming hasher for this upload
        self.hashers
            .lock()
            .expect("hasher lock poisoned")
            .insert(file_id.clone(), Sha256::new());

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
        let new_size =
            current_size
                .checked_add(data.len() as u64)
                .ok_or(StorageError::TooLarge {
                    size: u64::MAX,
                    limit: self.max_file_size,
                })?;
        if new_size > self.max_file_size {
            return Err(StorageError::TooLarge {
                size: new_size,
                limit: self.max_file_size,
            });
        }

        let mut file = fs::OpenOptions::new().append(true).open(&data_path).await?;
        file.write_all(&data).await?;
        file.flush().await?;

        // Update streaming hasher
        if let Some(hasher) = self
            .hashers
            .lock()
            .expect("hasher lock poisoned")
            .get_mut(file_id)
        {
            hasher.update(&data);
        }

        Ok(())
    }

    async fn finalize(&self, file_id: &str) -> Result<FileMetadata, StorageError> {
        let meta_path = self.meta_path(file_id);
        let meta_bytes = fs::read(&meta_path)
            .await
            .map_err(|_| StorageError::NotFound(file_id.to_owned()))?;
        let mut meta: MetaRecord = serde_json::from_slice(&meta_bytes)
            .map_err(|e| StorageError::Io(std::io::Error::other(e)))?;

        // Finalize the streaming hasher (falls back to file read if hasher missing)
        let checksum = if let Some(hasher) = self
            .hashers
            .lock()
            .expect("hasher lock poisoned")
            .remove(file_id)
        {
            hex::encode(hasher.finalize())
        } else {
            // Fallback: compute from file in streaming fashion (e.g. after restart)
            let mut file = fs::File::open(self.data_path(file_id)).await?;
            let mut hasher = Sha256::new();
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let n = file.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
            }
            hex::encode(hasher.finalize())
        };

        let size = fs::metadata(self.data_path(file_id)).await?.len();
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
        self.hashers
            .lock()
            .expect("hasher lock poisoned")
            .remove(file_id);
        remove_if_exists(self.data_path(file_id)).await?;
        remove_if_exists(self.meta_path(file_id)).await?;
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
        let file = fs::File::open(&data_path).await?;
        let reader_stream = ReaderStream::with_capacity(file, chunk_size);

        // Map io::Error to StorageError
        let mapped = tokio_stream::StreamExt::map(reader_stream, |result| {
            result.map_err(StorageError::from)
        });

        Ok(Box::new(mapped))
    }

    async fn delete(&self, file_id: &str) -> Result<(), StorageError> {
        // Verify exists
        if !self.meta_path(file_id).exists() {
            return Err(StorageError::NotFound(file_id.to_owned()));
        }
        remove_if_exists(self.data_path(file_id)).await?;
        remove_if_exists(self.meta_path(file_id)).await?;
        Ok(())
    }

    async fn list(&self) -> Result<Vec<FileMetadata>, StorageError> {
        let mut entries = fs::read_dir(&self.base_path).await?;
        let mut files = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let Some(ext) = path.extension() else {
                continue;
            };
            if ext != "meta" {
                continue;
            }

            let meta_bytes = fs::read(&path).await?;
            let Ok(meta) = serde_json::from_slice::<MetaRecord>(&meta_bytes) else {
                continue;
            };

            if !meta.finalized {
                continue;
            }

            files.push(FileMetadata {
                file_id: meta.file_id,
                file_name: meta.file_name,
                content_type: meta.content_type,
                size_bytes: meta.size_bytes,
                sha256_checksum: meta.sha256_checksum,
                uploaded_at: meta.uploaded_at,
            });
        }

        // Sort by upload time descending (newest first)
        files.sort_by(|a, b| b.uploaded_at.cmp(&a.uploaded_at));
        Ok(files)
    }
}

/// Remove a file, ignoring `NotFound` but propagating other IO errors.
async fn remove_if_exists(path: PathBuf) -> Result<(), StorageError> {
    match fs::remove_file(&path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(StorageError::from(e)),
    }
}
