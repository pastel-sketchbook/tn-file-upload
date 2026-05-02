use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bytes::Bytes;
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_stream::Stream;
use tokio_util::io::ReaderStream;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::pb::FileMetadata;

use super::{Storage, StorageError};

/// An in-flight hasher with its creation timestamp.
struct InFlightHasher {
    hasher: Sha256,
    created_at: Instant,
}

/// Local filesystem storage backend.
pub struct LocalStorage {
    base_path: PathBuf,
    max_file_size: u64,
    /// In-flight upload hashers, keyed by `file_id`.
    hashers: Arc<Mutex<HashMap<String, InFlightHasher>>>,
    /// Maximum time an upload can remain in-flight before the hasher is evicted.
    upload_ttl: Duration,
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
            hashers: Arc::new(Mutex::new(HashMap::new())),
            upload_ttl: Duration::from_mins(30),
        })
    }

    /// Spawn a background task that evicts stale in-flight hashers.
    ///
    /// Uploads that exceed `upload_ttl` without being finalized or aborted
    /// have their hasher removed (preventing unbounded memory growth).
    /// The task stops when `cancel` is cancelled.
    ///
    /// # Panics
    ///
    /// Panics if the internal hasher mutex is poisoned (indicates a prior panic).
    pub fn spawn_stale_upload_reaper(&self, cancel: CancellationToken) {
        let hashers = Arc::clone(&self.hashers);
        let ttl = self.upload_ttl;

        tokio::spawn(async move {
            let interval = ttl / 2; // sweep at half the TTL
            loop {
                tokio::select! {
                    () = cancel.cancelled() => break,
                    () = tokio::time::sleep(interval) => {}
                }

                // Invariant: lock is never held across an await.
                let evicted: Vec<String> = {
                    let mut map = hashers.lock().expect("hasher lock poisoned");
                    let now = Instant::now();
                    let stale: Vec<String> = map
                        .iter()
                        .filter(|(_, v)| now.duration_since(v.created_at) > ttl)
                        .map(|(k, _)| k.clone())
                        .collect();
                    for id in &stale {
                        map.remove(id);
                    }
                    stale
                };

                for id in &evicted {
                    tracing::warn!(file_id = %id, "evicted stale in-flight upload hasher (TTL exceeded)");
                }
            }
        });
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
        // Invariant: lock is never held across an await; poisoning implies a panic in another thread.
        self.hashers
            .lock()
            .expect("hasher lock poisoned")
            .insert(
                file_id.clone(),
                InFlightHasher {
                    hasher: Sha256::new(),
                    created_at: Instant::now(),
                },
            );

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
        // Invariant: lock is never held across an await; poisoning implies a panic in another thread.
        if let Some(entry) = self
            .hashers
            .lock()
            .expect("hasher lock poisoned")
            .get_mut(file_id)
        {
            entry.hasher.update(&data);
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
        // Invariant: lock is never held across an await; poisoning implies a panic in another thread.
        let checksum = if let Some(entry) = self
            .hashers
            .lock()
            .expect("hasher lock poisoned")
            .remove(file_id)
        {
            hex::encode(entry.hasher.finalize())
        } else {
            // Fallback: hasher missing (e.g. server restarted mid-upload). Re-hash from disk.
            // WARNING: this verifies on-disk integrity only, not what the client originally streamed.
            tracing::warn!(
                file_id = %file_id,
                "in-memory hasher missing; falling back to on-disk re-hash (integrity degraded)"
            );
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
        // Invariant: lock is never held across an await; poisoning implies a panic in another thread.
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
