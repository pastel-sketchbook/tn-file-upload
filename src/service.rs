use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};

use crate::pb::file_upload_server::FileUpload;
use crate::pb::{
    DeleteRequest, DeleteResponse, DownloadRequest, DownloadResponse, FileMetadata,
    GetMetadataRequest, UploadRequest, UploadResponse, upload_request,
};
use crate::storage::{Storage, StorageError};

/// gRPC service implementation for file upload/download.
pub struct FileUploadService {
    storage: Arc<dyn Storage>,
    chunk_size: usize,
}

impl FileUploadService {
    #[must_use]
    pub fn new(storage: Arc<dyn Storage>, chunk_size: usize) -> Self {
        Self {
            storage,
            chunk_size,
        }
    }
}

impl From<StorageError> for Status {
    fn from(err: StorageError) -> Self {
        match err {
            StorageError::NotFound(msg) => Status::not_found(msg),
            StorageError::TooLarge { size, limit } => {
                Status::resource_exhausted(format!("file size {size} exceeds limit {limit}"))
            }
            StorageError::InvalidFileName(msg) => Status::invalid_argument(msg),
            StorageError::Io(e) => {
                tracing::error!(error = %e, "storage I/O error");
                Status::internal("internal storage error")
            }
        }
    }
}

type DownloadStream = ReceiverStream<Result<DownloadResponse, Status>>;

#[tonic::async_trait]
impl FileUpload for FileUploadService {
    type DownloadStream = DownloadStream;

    /// Client-streaming upload: first message must be metadata, subsequent messages are chunks.
    #[tracing::instrument(skip(self, request), err)]
    async fn upload(
        &self,
        request: Request<Streaming<UploadRequest>>,
    ) -> Result<Response<UploadResponse>, Status> {
        self.handle_upload(request.into_inner()).await
    }

    /// Server-streaming download: streams file contents in chunks.
    #[tracing::instrument(skip(self, request), err)]
    async fn download(
        &self,
        request: Request<DownloadRequest>,
    ) -> Result<Response<Self::DownloadStream>, Status> {
        let file_id = request.into_inner().file_id;
        let chunk_stream = self
            .storage
            .read_chunks(&file_id, self.chunk_size)
            .await
            .map_err(Status::from)?;

        let (tx, rx) = mpsc::channel(128);

        tokio::spawn(async move {
            let mut chunk_stream = chunk_stream;
            while let Some(result) = chunk_stream.next().await {
                let msg = match result {
                    Ok(data) => Ok(DownloadResponse {
                        chunk: data.to_vec(),
                    }),
                    Err(e) => Err(Status::from(e)),
                };
                if tx.send(msg).await.is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    /// Unary: returns file metadata.
    #[tracing::instrument(skip(self, request), err)]
    async fn get_metadata(
        &self,
        request: Request<GetMetadataRequest>,
    ) -> Result<Response<FileMetadata>, Status> {
        let file_id = request.into_inner().file_id;
        let meta = self
            .storage
            .metadata(&file_id)
            .await
            .map_err(Status::from)?;
        Ok(Response::new(meta))
    }

    /// Unary: deletes a file.
    #[tracing::instrument(skip(self, request), err)]
    async fn delete(
        &self,
        request: Request<DeleteRequest>,
    ) -> Result<Response<DeleteResponse>, Status> {
        let file_id = request.into_inner().file_id;
        self.storage.delete(&file_id).await.map_err(Status::from)?;
        Ok(Response::new(DeleteResponse {}))
    }
}

impl FileUploadService {
    /// Internal upload handler that works with any stream (enables unit testing).
    ///
    /// # Errors
    ///
    /// Returns `Status` on invalid input, storage failures, or size limit violations.
    #[tracing::instrument(skip(self, stream))]
    pub async fn handle_upload(
        &self,
        mut stream: impl tokio_stream::Stream<Item = Result<UploadRequest, Status>> + Unpin + Send,
    ) -> Result<Response<UploadResponse>, Status> {
        // First message must be metadata
        let first = stream
            .next()
            .await
            .ok_or_else(|| Status::invalid_argument("empty upload stream"))?
            .map_err(|e| Status::internal(e.to_string()))?;

        let Some(upload_request::Request::Metadata(metadata)) = first.request else {
            return Err(Status::invalid_argument("first message must be metadata"));
        };

        let file_id = self
            .storage
            .create(&metadata.file_name, &metadata.content_type)
            .await
            .map_err(Status::from)?;

        // Stream chunks
        while let Some(msg) = stream.next().await {
            let msg = match msg {
                Ok(m) => m,
                Err(e) => {
                    self.storage.abort(&file_id).await.ok();
                    return Err(Status::internal(e.to_string()));
                }
            };

            match msg.request {
                Some(upload_request::Request::Chunk(data)) => {
                    if let Err(e) = self.storage.append(&file_id, data.into()).await {
                        self.storage.abort(&file_id).await.ok();
                        return Err(Status::from(e));
                    }
                }
                Some(upload_request::Request::Metadata(_)) => {
                    self.storage.abort(&file_id).await.ok();
                    return Err(Status::invalid_argument(
                        "metadata must only be sent as first message",
                    ));
                }
                None => {}
            }
        }

        // Finalize
        let meta = self
            .storage
            .finalize(&file_id)
            .await
            .map_err(Status::from)?;

        Ok(Response::new(UploadResponse {
            file_id: meta.file_id,
            size_bytes: meta.size_bytes,
            sha256_checksum: meta.sha256_checksum,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::local::LocalStorage;
    use bytes::Bytes;

    async fn test_service() -> FileUploadService {
        let dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(dir.into_path(), 1024 * 1024)
            .await
            .unwrap();
        FileUploadService::new(Arc::new(storage), 64 * 1024)
    }

    #[tokio::test]
    async fn upload_rejects_empty_stream() {
        let svc = test_service().await;
        let stream = tokio_stream::empty();
        let err = svc.handle_upload(stream).await.unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn upload_rejects_chunk_as_first_message() {
        let svc = test_service().await;
        let msg = UploadRequest {
            request: Some(upload_request::Request::Chunk(b"data".to_vec())),
        };
        let stream = tokio_stream::once(Ok(msg));
        let err = svc.handle_upload(stream).await.unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn upload_and_download_round_trip() {
        let svc = test_service().await;

        let messages = vec![
            Ok(UploadRequest {
                request: Some(upload_request::Request::Metadata(
                    crate::pb::UploadMetadata {
                        file_name: "test.txt".into(),
                        content_type: "text/plain".into(),
                    },
                )),
            }),
            Ok(UploadRequest {
                request: Some(upload_request::Request::Chunk(b"hello world".to_vec())),
            }),
        ];

        let stream = tokio_stream::iter(messages);
        let resp = svc.handle_upload(stream).await.unwrap().into_inner();

        assert_eq!(resp.size_bytes, 11);
        assert!(!resp.sha256_checksum.is_empty());

        // Download
        let dl_req = Request::new(DownloadRequest {
            file_id: resp.file_id.clone(),
        });
        let mut dl_stream = svc.download(dl_req).await.unwrap().into_inner();
        let mut downloaded = Vec::new();
        while let Some(chunk) = dl_stream.next().await {
            downloaded.extend_from_slice(&chunk.unwrap().chunk);
        }
        assert_eq!(downloaded, b"hello world");
    }

    #[tokio::test]
    async fn get_metadata_not_found() {
        let svc = test_service().await;
        let req = Request::new(GetMetadataRequest {
            file_id: "nonexistent".into(),
        });
        let err = svc.get_metadata(req).await.unwrap_err();
        assert_eq!(err.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn delete_not_found() {
        let svc = test_service().await;
        let req = Request::new(DeleteRequest {
            file_id: "nonexistent".into(),
        });
        let err = svc.delete(req).await.unwrap_err();
        assert_eq!(err.code(), tonic::Code::NotFound);
    }
}
