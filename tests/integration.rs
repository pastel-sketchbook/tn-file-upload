use std::sync::Arc;

use tokio::net::TcpListener;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::Request;
use tonic::transport::Server;

use tn_file_upload::pb::file_upload_client::FileUploadClient;
use tn_file_upload::pb::file_upload_server::FileUploadServer;
use tn_file_upload::pb::{
    DeleteRequest, DownloadRequest, GetMetadataRequest, UploadMetadata, UploadRequest,
    upload_request,
};
use tn_file_upload::service::FileUploadService;
use tn_file_upload::storage::local::LocalStorage;

async fn spawn_server() -> String {
    let dir = tempfile::tempdir().unwrap();
    let storage = LocalStorage::new(dir.into_path(), 1024 * 1024)
        .await
        .unwrap();
    let service = FileUploadService::new(Arc::new(storage), 64 * 1024);

    let listener = TcpListener::bind("[::1]:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        Server::builder()
            .add_service(FileUploadServer::new(service))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    format!("http://[::1]:{port}")
}

#[tokio::test]
async fn full_upload_download_delete_cycle() {
    let addr = spawn_server().await;
    let mut client = FileUploadClient::connect(addr).await.unwrap();

    // Upload
    let messages = vec![
        UploadRequest {
            request: Some(upload_request::Request::Metadata(UploadMetadata {
                file_name: "integration.txt".into(),
                content_type: "text/plain".into(),
            })),
        },
        UploadRequest {
            request: Some(upload_request::Request::Chunk(
                b"integration test data".to_vec(),
            )),
        },
    ];

    let resp = client
        .upload(Request::new(tokio_stream::iter(messages)))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.size_bytes, 21);
    assert!(!resp.file_id.is_empty());

    // Get metadata
    let meta = client
        .get_metadata(Request::new(GetMetadataRequest {
            file_id: resp.file_id.clone(),
        }))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(meta.file_name, "integration.txt");
    assert_eq!(meta.size_bytes, 21);

    // Download
    let mut stream = client
        .download(Request::new(DownloadRequest {
            file_id: resp.file_id.clone(),
        }))
        .await
        .unwrap()
        .into_inner();

    let mut downloaded = Vec::new();
    while let Some(chunk) = stream.next().await {
        downloaded.extend_from_slice(&chunk.unwrap().chunk);
    }
    assert_eq!(downloaded, b"integration test data");

    // Delete
    client
        .delete(Request::new(DeleteRequest {
            file_id: resp.file_id.clone(),
        }))
        .await
        .unwrap();

    // Verify gone
    let err = client
        .get_metadata(Request::new(GetMetadataRequest {
            file_id: resp.file_id,
        }))
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::NotFound);
}
