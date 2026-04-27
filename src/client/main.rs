use std::path::PathBuf;

use anyhow::{Context, Result};
use tokio::fs;
use tokio_stream::StreamExt;
use tonic::Request;

use tn_file_upload::pb::file_upload_client::FileUploadClient;
use tn_file_upload::pb::{
    DownloadRequest, GetMetadataRequest, UploadMetadata, UploadRequest, upload_request,
};

#[tokio::main]
async fn main() -> Result<()> {
    let addr = std::env::var("SERVER_ADDR").unwrap_or_else(|_| "http://[::1]:50051".into());
    let token = std::env::var("AUTH_TOKEN").unwrap_or_else(|_| "dev-token".into());

    let file_path = std::env::args()
        .nth(1)
        .context("usage: file-upload-client <file-path>")?;

    let mut client = FileUploadClient::connect(addr)
        .await
        .context("connecting")?;

    // Upload
    let path = PathBuf::from(&file_path);
    let file_name = path
        .file_name()
        .context("invalid file path")?
        .to_string_lossy()
        .to_string();
    let data = fs::read(&path).await.context("reading file")?;

    let metadata_msg = UploadRequest {
        request: Some(upload_request::Request::Metadata(UploadMetadata {
            file_name,
            content_type: "application/octet-stream".into(),
        })),
    };

    let chunks: Vec<UploadRequest> = data
        .chunks(64 * 1024)
        .map(|c| UploadRequest {
            request: Some(upload_request::Request::Chunk(c.to_vec())),
        })
        .collect();

    let mut messages = vec![metadata_msg];
    messages.extend(chunks);

    let mut req = Request::new(tokio_stream::iter(messages));
    req.metadata_mut()
        // ASCII token always parses to valid metadata value
        .insert("x-auth-token", token.parse().expect("valid ASCII token"));

    let resp = client.upload(req).await.context("uploading")?.into_inner();
    println!(
        "Uploaded: id={}, size={}, checksum={}",
        resp.file_id, resp.size_bytes, resp.sha256_checksum
    );

    // Get metadata
    let mut meta_req = Request::new(GetMetadataRequest {
        file_id: resp.file_id.clone(),
    });
    meta_req
        .metadata_mut()
        // ASCII token always parses to valid metadata value
        .insert("x-auth-token", token.parse().expect("valid ASCII token"));
    let meta = client
        .get_metadata(meta_req)
        .await
        .context("get_metadata")?
        .into_inner();
    println!("Metadata: {meta:?}");

    // Download
    let mut dl_req = Request::new(DownloadRequest {
        file_id: resp.file_id,
    });
    dl_req
        .metadata_mut()
        // ASCII token always parses to valid metadata value
        .insert("x-auth-token", token.parse().expect("valid ASCII token"));
    let mut stream = client
        .download(dl_req)
        .await
        .context("downloading")?
        .into_inner();

    let mut downloaded = Vec::new();
    while let Some(chunk) = stream.next().await {
        downloaded.extend_from_slice(&chunk.context("download chunk")?.chunk);
    }
    println!("Downloaded {} bytes", downloaded.len());

    Ok(())
}
