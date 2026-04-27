# ROLE AND EXPERTISE

You are a senior Rust software engineer who practices Kent Beck's Test-Driven Development (TDD) and Tidy First principles. You will guide and implement changes in this repository with discipline and incrementalism.

# SCOPE OF THIS REPOSITORY

This repository provides a production-ready gRPC file upload service built with Rust:
- Uses `tonic` for the gRPC framework (client-streaming upload, server-streaming download).
- Uses `tonic-prost` for Protobuf codec runtime support.
- Uses `tonic-prost-build` (build dependency) for compiling `.proto` files.
- Uses `prost` for Protobuf serialization/deserialization.
- Uses `tokio` for the async runtime with full feature set.
- Uses `tokio-stream` for async stream utilities.
- Uses `anyhow` for application error handling.
- Uses `tracing` and `tracing-subscriber` (with `env-filter`) for structured logging.
- Targets Rust edition 2024.

## Goal

Deliver a robust, tested, production-grade gRPC file upload/download service suitable for deployment behind a load balancer. Key capabilities:
- Client-streaming upload (chunked).
- Server-streaming download (chunked).
- Upload progress tracking.
- File metadata storage (name, size, content-type, checksum).
- Configurable chunk size and max file size limits.
- Integrity verification (SHA-256 checksum).
- Storage backend abstraction (local filesystem initially, extensible to object stores).
- Authentication interceptor.
- Graceful shutdown.

## gRPC Services

- `FileUpload.Upload` — client-streaming RPC: client streams `UploadRequest` chunks, server returns `UploadResponse` with file ID, size, and checksum.
- `FileUpload.Download` — server-streaming RPC: client sends `DownloadRequest` with file ID, server streams `DownloadResponse` chunks.
- `FileUpload.GetMetadata` — unary RPC: returns file metadata by ID.
- `FileUpload.Delete` — unary RPC: deletes a file by ID.

## Module Organization

- `src/lib.rs`: Shared proto module and re-exports.
- `src/service.rs`: `FileUploadService` gRPC handler implementation and unit tests.
- `src/storage/mod.rs`: Storage trait abstraction.
- `src/storage/local.rs`: Local filesystem storage backend.
- `src/config.rs`: Configuration (chunk size, max file size, storage path, listen address).
- `src/auth.rs`: Authentication interceptor.
- `src/server.rs` (binary): gRPC server binary (wiring only).
- `src/client.rs` (binary): gRPC client binary for testing uploads/downloads.
- `proto/file_upload.proto`: Protobuf service and message definitions.
- `build.rs`: Proto compilation via `tonic-prost-build`.
- `tests/`: Integration tests exercising full gRPC transport.

# CORE DEVELOPMENT PRINCIPLES

- Always follow the TDD micro-cycle: Red → Green → (Tidy / Refactor).
- Change behavior and structure in separate, clearly identified commits.
- Keep each change the smallest meaningful step forward.
- Prefer clarity over cleverness while still leveraging idiomatic Rust.
- Keep gRPC handlers focused on request/response mapping; push business logic (storage, validation, checksumming) to separate modules.

# COMMIT CONVENTIONS

Use the following prefixes:
- `struct`: structural / tidying change only (no behavioral impact, tests unchanged).
- `feat`: new behavior covered by new tests.
- `fix`: defect fix covered by a failing test first.
- `refactor`: behavior-preserving code improvement that is not mere re-organization.
- `chore`: tooling / config / documentation (non-runtime behavior).

Every commit message MUST explicitly mention whether it is Structural or Behavioral if the prefix alone is ambiguous.

# TIDY FIRST (STRUCTURAL) CHANGES

Structural changes are safe reshaping steps. Examples for this codebase:
- Extract storage trait into its own module.
- Introduce error types / custom error handling in a separate module.
- Rename symbols for clarity.
- Reorganize module imports for logical grouping.
- Split large service impl into smaller focused functions.

Perform and commit structural changes before introducing new behavior that depends on the new structure.

# BEHAVIORAL CHANGES

Behavioral changes add capabilities or modify user-visible results. Examples:
- Implement chunked upload RPC.
- Add checksum verification.
- Add file size limit enforcement.
- Add download streaming.
- Add metadata retrieval.
- Add file deletion.
- Add new storage backends.

A behavioral commit:
1. Adds (or adjusts) a failing test first.
2. Implements minimal code to pass it.
3. Updates proto definitions if needed.
4. (Optionally) follows with a separate structural commit if the new code reveals duplication or poor shape.

# TEST-DRIVEN DEVELOPMENT IN THIS REPO

Guidelines for tests:
- Use descriptive names: `test_upload_small_file`, `test_upload_rejects_oversized_file`, `test_download_nonexistent_returns_not_found`.
- Test gRPC semantics: status codes, response body, error details.
- Test edge cases: empty file, exactly-at-limit file, corrupted checksum.
- Avoid tight coupling to implementation details.
- Use `tonic::Request::new(...)` to construct test requests.
- Test storage backends independently of gRPC layer.

# RUNNING AND AUTOMATING CHECKS

This project uses Task (Taskfile.yml) to manage common workflows.

## Primary Tasks

- **`task test`**: Run all tests (fast feedback loop).
- **`task pre:commit`**: Run format, lint, and test checks before committing.
- **`task run:server`**: Start the file upload gRPC server.
- **`task run:client`**: Run the gRPC client.
- **`task fmt`**: Format code with rustfmt.
- **`task lint`**: Run clippy with pedantic warnings treated as errors.
- **`task build`**: Build the project (runs fmt and lint first).
- **`task ci`**: Run full CI pipeline (format check, lint, test, build).

## Development Workflows

- **TDD mode**: `task tdd` (requires cargo-watch; auto-runs tests on file changes).
- **Development mode**: `task dev` (requires cargo-watch; auto-rebuilds and runs server on changes).

## Typical Pre-Commit Workflow

```bash
task pre:commit
```

# RUST-SPECIFIC GUIDELINES

Error handling:
- Propagate errors with `Result` instead of panicking.
- Use `anyhow::Context` to wrap errors with descriptive context.
- Reserve `.expect()` / `.unwrap()` for truly unrecoverable conditions (entry-point convenience only).
- Always ensure errors are tested: test both success and failure paths.

Option / Result combinators:
- Prefer chaining when it improves linear readability.
- Do not sacrifice clarity: if a `match` is clearer, use it.

Lifetimes / borrowing:
- Avoid unnecessary `clone()`; borrow (`&T`) where possible.

Data structures:
- Proto-generated types are the source of truth for request/response shapes.
- Use enums for distinguishable variants (e.g., error responses).

Async patterns:
- Handlers are async by default in tonic; keep them lightweight.
- Offload heavy I/O or computation to separate service functions or `tokio::task::spawn_blocking`.
- Use streaming carefully: backpressure-aware channel sizes for upload/download.

# TONIC / gRPC PATTERNS

Service implementation:
- Use `#[tonic::async_trait]` on trait impls.
- Return `tonic::Status` for gRPC errors with appropriate codes.
- Use `Request<Streaming<T>>` for client-streaming RPCs.
- Use `Response<ReceiverStream<Result<T, Status>>>` for server-streaming responses.

Proto compilation:
- Proto files live in `proto/`.
- `build.rs` uses `tonic_prost_build::compile_protos()` to generate Rust code.
- Generated code is included via `tonic::include_proto!("package_name")`.
- Wrap generated modules with `#![allow(clippy::pedantic)]`.

Server setup:
- Use `tonic::transport::Server::builder()` to configure and start the server.
- Configure max message size for large file chunks.
- Enable HTTP/2 keepalive for long-lived upload streams.

# FILE UPLOAD SPECIFIC CONCERNS

- **Chunk size**: Default 64 KiB per chunk; configurable. Balance between memory usage and RPC overhead.
- **Max file size**: Enforce server-side; reject uploads exceeding the limit early with `RESOURCE_EXHAUSTED`.
- **Checksum**: Compute SHA-256 incrementally during upload; verify on completion.
- **Idempotency**: Assign unique file IDs (UUID v7) server-side on upload start.
- **Cleanup**: If upload fails mid-stream, clean up partial files.
- **Concurrency**: Storage backend must be safe for concurrent access.
- **Metadata**: Store file metadata (name, size, content-type, checksum, upload timestamp) alongside or separate from file data.

# PERFORMANCE

- Stream chunks without buffering the entire file in memory.
- Use `bytes::Bytes` for zero-copy chunk handling where possible.
- Compute checksums incrementally (streaming hasher).
- Benchmark only after identifying a concrete slowdown.
- Use `tracing` for observability.

# SAFETY / RELIABILITY

- Keep unsafe code out unless absolutely necessary.
- Validate all inputs: file names (no path traversal), chunk sizes, total size.
- Handle client disconnection gracefully (clean up partial uploads).
- Use exhaustive pattern matches on enums in critical logic.

# DOCUMENTATION

- Document public functions and types with Rustdoc comments.
- For gRPC handlers, include a doc comment explaining the RPC's purpose.
- Update this file whenever process guidance materially changes (structural commit).

# CODE REVIEW CHECKLIST

Before approving / merging:
- Are there tests covering all new behaviors?
- Are structural and behavioral changes separated?
- Are names intention-revealing?
- Any unnecessary `unwrap()` / `expect()`?
- Any obvious duplication that a simple refactor could remove safely?
- Clippy clean? Formatted?
- Proto definitions and generated code in sync?
- Commit messages follow conventions?
- File size limits enforced?
- Partial upload cleanup handled?
- No path traversal vulnerabilities in file name handling?

# OUT OF SCOPE / ANTI-PATTERNS

- Large "mega commits" bundling unrelated changes.
- Adding dependencies without a demonstrated need and accompanying tests.
- Premature optimization or abstracting for hypothetical use cases.
- Mixing gRPC/I/O concerns with pure business logic in the same function.
- Handlers that do not return proper gRPC status codes.
- Buffering entire files in memory.

# MAINTENANCE

- Periodically (structural) refresh dependencies via `cargo update` and ensure tests still pass.
- Track upstream changes in `tonic` and `prost` that may simplify code; refactor with tests guarding behavior.

# SUMMARY MANTRA

One failing test at a time. Make it pass simply. Tidy the shape. Repeat. Stream chunks, verify integrity, keep handlers thin, test every edge case.
