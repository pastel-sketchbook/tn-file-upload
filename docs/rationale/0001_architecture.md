# 0001 — Architecture: gRPC File Upload Service with Tonic

## Status

Accepted.

## Context

We need a production-grade file upload/download service that can sit behind a load balancer, handle large files without exhausting memory, verify data integrity, and remain extensible to different storage backends. The service must be observable, authenticated, and ready for Kubernetes deployment.

This document explains *why* the architecture is shaped the way it is, not just *what* it does.

## Decision: gRPC over REST

File upload over REST typically means multipart form encoding, which is awkward for large files: the entire body must arrive before the server can process it (or you implement chunked transfer encoding manually). gRPC's native streaming support solves this at the protocol level.

- **Client-streaming upload**: the client opens a single HTTP/2 stream and sends chunks as individual protobuf messages. The server processes each chunk as it arrives — no buffering the entire file.
- **Server-streaming download**: the inverse. The server reads the file in chunks and streams them back. The client reassembles.
- **HTTP/2 multiplexing**: multiple uploads/downloads can share a single TCP connection without head-of-line blocking.

The trade-off is that gRPC is less accessible than REST for browser clients (requires grpc-web proxy), but this service targets backend-to-backend or CLI-to-backend use cases where that is not a constraint.

## Decision: Tonic as the gRPC framework

Tonic is the de facto gRPC framework for Rust. The choice is straightforward:

- Built on `tokio` and `hyper` — the same async ecosystem the rest of the Rust web world uses.
- First-class streaming support via `Streaming<T>` (inbound) and `ReceiverStream` (outbound).
- Interceptor API for cross-cutting concerns (auth, tracing).
- `tonic-health` crate for standard gRPC health checking protocol.
- Code generation via `tonic-prost-build` produces idiomatic Rust from `.proto` files.

We use `tonic-prost-build` rather than `tonic-build` directly because it bundles the `prost` codec, keeping `build.rs` to a single line.

## Architecture overview

```
┌───────────────────────────────────────────────────────────────┐
│                        gRPC Server                            │
│                                                               │
│  ┌────────────────┐    ┌──────────────┐    ┌───────────────┐  │
│  │  Request-ID    │───▶│     Auth     │───▶│  FileUpload   │  │
│  │ Interceptor    │    │ Interceptor  │    │   Service     │  │
│  └────────────────┘    └──────────────┘    └───────┬───────   │
│                                                    │          │
│                                          ┌─────────▼───────┐  │
│                                          │ Storage Trait   │  │
│                                          └─────────┬───────┘  │
│                                                    │          │
│                                          ┌─────────▼───────┐  │
│  ┌──────────────┐                        │ LocalStorage    │  │
│  │ tonic-health │                        │  (filesystem)   │  │
│  │   Service    │                        └─────────────────┘  │
│  └──────────────┘                                             │
└───────────────────────────────────────────────────────────────┘
```

The server is composed of three layers:

1. **Interceptors** — run before every RPC. Request-ID goes first (always succeeds, adds trace context), then auth (may reject).
2. **Service** — the `FileUploadService` struct implements the generated `FileUpload` trait. It owns an `Arc<dyn Storage>` and a chunk size.
3. **Storage** — the `Storage` trait abstracts file persistence. `LocalStorage` implements it with the local filesystem.

The health service runs as a separate gRPC service on the same port, independent of the upload service.

## The upload protocol

The upload RPC uses a `oneof` message design:

```protobuf
message UploadRequest {
  oneof request {
    UploadMetadata metadata = 1;
    bytes chunk = 2;
  }
}
```

The first message in the stream **must** be `UploadMetadata` (file name, content type). All subsequent messages are raw byte chunks. This is enforced server-side with an early return if the contract is violated.

Why `oneof` instead of separate RPCs or a header-then-body convention?

- **Type safety**: the protobuf schema makes the protocol self-documenting. A `UploadMetadata` message cannot be confused with a chunk at the wire level.
- **Single stream**: metadata and data flow over one RPC call. No need to coordinate a "create" RPC followed by a "push chunks" RPC — fewer round trips, simpler client logic.
- **Clean error handling**: if the server rejects the metadata (bad file name, path traversal), the stream terminates immediately. The client does not waste bandwidth sending chunks that will be discarded.

The server assigns a UUID v7 file ID on receiving metadata, then appends chunks to storage as they arrive. On stream completion, it finalizes the upload (computes checksum, records size, timestamps). If the stream errors mid-flight, the server aborts and cleans up partial data.

## Storage trait design

```rust
#[async_trait]
pub trait Storage: Send + Sync + 'static {
    async fn create(&self, ...) -> Result<String, StorageError>;
    async fn append(&self, ...) -> Result<(), StorageError>;
    async fn finalize(&self, ...) -> Result<FileMetadata, StorageError>;
    async fn abort(&self, ...) -> Result<(), StorageError>;
    async fn metadata(&self, ...) -> Result<FileMetadata, StorageError>;
    async fn read_chunks(&self, ...) -> Result<Box<dyn Stream<...>>, StorageError>;
    async fn delete(&self, ...) -> Result<(), StorageError>;
}
```

The trait uses `#[async_trait]` because the service holds `Arc<dyn Storage>` — native async fn in traits is not yet dyn-compatible in stable Rust. Once it is, we drop the macro.

The lifecycle is explicit: `create` → `append`* → `finalize` (success) or `abort` (failure). This maps directly to the upload stream lifecycle and makes partial cleanup straightforward:

- `create` allocates the file ID and empty data file.
- `append` writes chunks incrementally, checking size limits on each call.
- `finalize` computes the SHA-256 checksum over the complete file and writes metadata.
- `abort` removes both data and metadata files.

The `read_chunks` method returns a `Box<dyn Stream>` rather than loading the entire file into memory. For the local backend this currently reads the whole file then chunks it (acceptable for local disk), but an object store backend would stream directly from the remote.

### Why not a single `upload(metadata, stream)` method?

Keeping `create`/`append`/`finalize` separate gives the service layer control over the lifecycle. The service can:

- Enforce size limits incrementally (reject at chunk N, not after buffering everything).
- Clean up on any error at any stage.
- Log progress per chunk if needed.
- Be tested at the storage level independently of gRPC streaming.

## Error mapping

Storage errors map to gRPC status codes via a `From<StorageError> for Status` implementation:

| `StorageError` | gRPC Status |
|---|---|
| `NotFound` | `NOT_FOUND` |
| `TooLarge` | `RESOURCE_EXHAUSTED` |
| `InvalidFileName` | `INVALID_ARGUMENT` |
| `Io` | `INTERNAL` (details logged server-side, not leaked to client) |

This follows the tonic patterns rule: never expose internal error details in `Status::internal` messages. The real error is logged via `tracing::error!` before the sanitized status is returned.

## Interceptor composition

Tonic's `with_interceptor` accepts a single function. We compose two interceptors by chaining:

```rust
fn combined_interceptor(req: Request<()>) -> Result<Request<()>, Status> {
    let req = request_id_interceptor(req)?;
    auth_interceptor(req)
}
```

Order matters:

1. **Request-ID first** — always succeeds, injects a UUID v7 into request extensions and the tracing span. Even rejected requests get a trace ID in the logs.
2. **Auth second** — may reject with `UNAUTHENTICATED`. The rejection log line includes the request ID from step 1.

The request-ID interceptor propagates an inbound `x-request-id` header if present (for distributed tracing), otherwise generates a new UUID v7. UUID v7 is time-ordered, so log lines sort chronologically by ID — useful for debugging without a centralized trace collector.

## Health checking

The `tonic-health` crate implements the [gRPC Health Checking Protocol](https://github.com/grpc/grpc/blob/master/doc/health-checking.md). Kubernetes can probe it directly with `grpc` liveness/readiness probes (no HTTP adapter needed).

The health service runs a background task that monitors `AppState.healthy` (an `AtomicBool`) and flips the serving status accordingly. This is intentionally simple — the health flag can be driven by:

- Storage backend connectivity checks.
- Memory pressure signals.
- Manual operator toggle via admin endpoint.

The health service is registered as a separate `add_service` on the same `Server::builder()`. It shares no interceptors with the upload service — health probes should not require authentication.

## Graceful shutdown

The server uses `serve_with_incoming_shutdown` with a `ctrl_c()` future. When the signal arrives:

1. Tonic sends HTTP/2 GOAWAY to all connected clients.
2. In-flight RPCs (including active uploads) are allowed to complete.
3. No new connections are accepted.
4. The server future resolves and `main()` returns.

For a production deployment with background tasks (e.g., periodic cleanup of orphaned uploads), a `CancellationToken` from `tokio-util` would coordinate shutdown across all tasks. The current implementation is intentionally minimal — there are no background tasks yet.

## Security considerations

### Authentication

The auth interceptor uses a shared token (`AUTH_TOKEN` env var) validated via `LazyLock`. In debug builds, it defaults to `dev-token`; in release builds, the process panics at startup if the variable is unset. This is a deliberate fail-fast: a production service without auth is worse than a crashed service.

A real deployment would replace this with JWT validation, mTLS, or an external auth service. The interceptor pattern makes this a drop-in replacement.

### Path traversal

`LocalStorage::validate_file_name` rejects any file name containing non-`Normal` path components (`..`, `/`, absolute paths). This prevents a malicious client from writing outside the storage directory. The check runs before the file ID is assigned — no resources are allocated for invalid requests.

### Size limits

File size is enforced incrementally during `append`, not after the upload completes. This means an oversized upload is rejected as soon as it exceeds the limit, not after the client has streamed gigabytes of data. The partial file is cleaned up via `abort`.

## Testability

The service is tested at three levels:

1. **Unit tests** — call `handle_upload` (an internal method accepting any `impl Stream`) and the trait methods (`download`, `get_metadata`, `delete`) directly. No network, no transport overhead.
2. **Storage tests** — (not yet extracted, but the storage trait enables them) test `LocalStorage` independently of the gRPC layer.
3. **Integration tests** — spin up a real tonic server on a random port (`[::1]:0`), connect a client, and exercise the full upload → metadata → download → delete cycle over the wire.

The `handle_upload` extraction is a deliberate design choice. Tonic's `upload` method requires `Request<Streaming<UploadRequest>>`, which cannot be constructed in unit tests without a real transport. By extracting the logic into a method that accepts `impl Stream<Item = Result<UploadRequest, Status>>`, we can test with `tokio_stream::iter` — fast, deterministic, no network.

## Benchmarks

Criterion benchmarks measure three things:

- **Upload throughput** — how fast can we receive and persist chunks? This exercises the full `handle_upload` path including SHA-256 computation and filesystem writes.
- **Download throughput** — how fast can we read and stream chunks? This exercises `read_chunks` and the `ReceiverStream` channel.
- **Raw storage append** — isolated filesystem write performance without gRPC overhead. This establishes the baseline: any slowness in upload benchmarks beyond this number is gRPC/proto overhead.

File sizes range from 1 KB to 1 MB. Larger files would be more realistic but make the benchmark suite slow. The shape of the curve (how time scales with size) matters more than absolute numbers for identifying algorithmic issues.

## What this architecture does NOT do (yet)

- **Resumable uploads** — a dropped connection loses the upload. Adding resumability would require server-side session tracking and a `ResumeUpload` RPC.
- **Concurrent chunk writes** — chunks are appended sequentially. For an object store backend, parallel part uploads (like S3 multipart) would improve throughput.
- **Compression** — gRPC supports transparent compression, but it is not enabled. For already-compressed files (images, video), it would waste CPU.
- **Rate limiting** — no per-client upload rate control. Would be implemented as a tower middleware layer.
- **Object store backends** — the `Storage` trait is ready for S3/GCS/Azure Blob implementations, but only `LocalStorage` ships today.

## Consequences

- The streaming-first design means memory usage stays bounded regardless of file size.
- The `Storage` trait inversion means adding a new backend (S3, GCS) requires zero changes to the service layer.
- The interceptor chain is simple to extend (add rate limiting, request logging, etc.) without touching service logic.
- The test pyramid (unit → integration) catches both logic bugs and wire protocol issues.
- The `async-trait` dependency will be removable once Rust stabilizes dyn-compatible async trait methods.
