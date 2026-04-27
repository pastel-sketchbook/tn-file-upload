use std::sync::Arc;

use bytes::Bytes;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use tokio::runtime::Runtime;
use tokio_stream::StreamExt;
use tonic::Request;

use tn_file_upload::pb::file_upload_server::FileUpload;
use tn_file_upload::pb::{DownloadRequest, UploadMetadata, UploadRequest, upload_request};
use tn_file_upload::service::FileUploadService;
use tn_file_upload::storage::Storage;
use tn_file_upload::storage::local::LocalStorage;

/// Create a service backed by a fresh temp directory.
async fn setup_service() -> (FileUploadService, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let storage = LocalStorage::new(dir.path(), 256 * 1024 * 1024)
        .await
        .unwrap();
    let svc = FileUploadService::new(Arc::new(storage), 64 * 1024);
    (svc, dir)
}

/// Build an upload stream from a data payload.
fn upload_messages(
    file_name: &str,
    data: &[u8],
    chunk_size: usize,
) -> Vec<Result<UploadRequest, tonic::Status>> {
    let mut msgs = vec![Ok(UploadRequest {
        request: Some(upload_request::Request::Metadata(UploadMetadata {
            file_name: file_name.into(),
            content_type: "application/octet-stream".into(),
        })),
    })];

    for chunk in data.chunks(chunk_size) {
        msgs.push(Ok(UploadRequest {
            request: Some(upload_request::Request::Chunk(chunk.to_vec())),
        }));
    }

    msgs
}

/// Benchmark: upload files of varying sizes.
fn bench_upload(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("upload");

    for size_kb in [1, 64, 256, 1024] {
        let data = vec![0xABu8; size_kb * 1024];

        group.bench_with_input(BenchmarkId::new("size_kb", size_kb), &data, |b, data| {
            b.to_async(&rt).iter_with_setup(
                || {
                    let data = data.clone();
                    async move {
                        let (svc, _dir) = setup_service().await;
                        let msgs = upload_messages("bench.bin", &data, 64 * 1024);
                        (svc, _dir, msgs)
                    }
                },
                |setup_future| async move {
                    let (svc, _dir, msgs) = setup_future.await;
                    let stream = tokio_stream::iter(msgs);
                    svc.handle_upload(stream).await.unwrap();
                },
            );
        });
    }

    group.finish();
}

/// Benchmark: download files of varying sizes.
fn bench_download(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("download");

    for size_kb in [1, 64, 256, 1024] {
        let data = vec![0xCDu8; size_kb * 1024];

        group.bench_with_input(BenchmarkId::new("size_kb", size_kb), &data, |b, data| {
            b.to_async(&rt).iter_with_setup(
                || {
                    let data = data.clone();
                    async move {
                        let (svc, _dir) = setup_service().await;
                        let msgs = upload_messages("bench.bin", &data, 64 * 1024);
                        let stream = tokio_stream::iter(msgs);
                        let resp = svc.handle_upload(stream).await.unwrap();
                        let file_id = resp.into_inner().file_id;
                        (svc, _dir, file_id)
                    }
                },
                |setup_future| async move {
                    let (svc, _dir, file_id) = setup_future.await;
                    let req = Request::new(DownloadRequest { file_id });
                    let mut stream = svc.download(req).await.unwrap().into_inner();
                    while let Some(chunk) = stream.next().await {
                        let _ = chunk.unwrap();
                    }
                },
            );
        });
    }

    group.finish();
}

/// Benchmark: raw storage append throughput (no gRPC overhead).
fn bench_storage_append(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("storage_append");

    let chunk = Bytes::from(vec![0xEFu8; 64 * 1024]);

    for num_chunks in [1, 16, 64, 256] {
        group.bench_with_input(
            BenchmarkId::new("chunks", num_chunks),
            &num_chunks,
            |b, &num_chunks| {
                let chunk = chunk.clone();
                b.to_async(&rt).iter_with_setup(
                    || {
                        let chunk = chunk.clone();
                        async move {
                            let dir = tempfile::tempdir().unwrap();
                            let storage = LocalStorage::new(dir.path(), 256 * 1024 * 1024)
                                .await
                                .unwrap();
                            let file_id = storage
                                .create("bench.bin", "application/octet-stream")
                                .await
                                .unwrap();
                            (storage, dir, file_id, chunk)
                        }
                    },
                    |setup_future| async move {
                        let (storage, _dir, file_id, chunk) = setup_future.await;
                        for _ in 0..num_chunks {
                            storage.append(&file_id, chunk.clone()).await.unwrap();
                        }
                        storage.finalize(&file_id).await.unwrap();
                    },
                );
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_upload, bench_download, bench_storage_append);
criterion_main!(benches);
