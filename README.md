# tn-file-upload

A production-ready gRPC file upload/download service built with Rust and [Tonic](https://github.com/hyperium/tonic).

## Features

- **Chunked upload** — client-streaming RPC; files are streamed in configurable chunks without buffering the entire file in memory.
- **Chunked download** — server-streaming RPC; files are read via `ReaderStream` and streamed back in chunks (zero full-file buffering).
- **REST API** — axum-based HTTP endpoints (`/api/upload`, `/api/files`, `/api/files/{id}/download`) for browser SPAs that cannot use gRPC client-streaming.
- **SHA-256 integrity verification** — checksums are computed incrementally during upload via an in-memory streaming hasher and returned on completion.
- **File metadata** — name, size, content type, checksum, and upload timestamp stored alongside file data.
- **File deletion** — remove files and associated metadata by ID.
- **Storage abstraction** — pluggable `Storage` trait; ships with a local filesystem backend.
- **Authentication** — constant-time token validation (`subtle::ConstantTimeEq`) on both gRPC (`x-auth-token` metadata) and REST (`x-auth-token` header). Token injected from config (no global state).
- **Request tracing** — UUID v7 request IDs injected via interceptor; all RPC methods instrumented with `tracing`.
- **Health checks** — `tonic-health` gRPC health service for Kubernetes liveness/readiness probes.
- **Graceful shutdown** — shared `CancellationToken` wires `SIGINT` to both gRPC, REST, and the health monitor; in-flight requests complete before exit.
- **Stale upload reaper** — background task evicts abandoned in-flight upload hashers after a configurable TTL (default 30 min), preventing unbounded memory growth.
- **Configurable limits** — max file size and chunk size controlled via environment variables.

## gRPC API

| RPC | Type | Description |
|-----|------|-------------|
| `Upload` | Client-streaming | Stream file chunks; returns file ID, size, and checksum |
| `Download` | Server-streaming | Request file by ID; receive chunks |
| `GetMetadata` | Unary | Retrieve file metadata by ID |
| `Delete` | Unary | Delete a file by ID |

Proto definition: [`proto/file_upload.proto`](proto/file_upload.proto)

## REST API

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/api/upload` | Multipart file upload (streamed chunk-by-chunk) |
| `GET` | `/api/files` | List all uploaded files |
| `GET` | `/api/files/{id}` | Get file metadata |
| `GET` | `/api/files/{id}/download` | Stream file download |
| `DELETE` | `/api/files/{id}` | Delete a file |

All REST endpoints require the `x-auth-token` header.

## Quick start

### Prerequisites

- Rust 1.95+ (edition 2024)
- [Task](https://taskfile.dev/) (optional, for workflow commands)
- [protoc](https://grpc.io/docs/protoc-installation/) (Protocol Buffers compiler)

### Run the server

```bash
cp .env.example .env   # edit AUTH_TOKEN
export $(cat .env | xargs)
task run:server
# or: cargo run --bin file-upload-server
```

### Run the client

```bash
export AUTH_TOKEN=your-token SERVER_ADDR=http://[::1]:50051
task run:client -- path/to/file.bin
# or: cargo run --bin file-upload-client -- path/to/file.bin
```

## Configuration

All configuration is via environment variables. See [`.env.example`](.env.example) for the full list.

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `AUTH_TOKEN` | Yes | `dev-token` (debug only) | Authentication token |
| `LISTEN_ADDR` | No | `[::]:50051` | gRPC server bind address |
| `REST_ADDR` | No | `[::]:3001` | REST API bind address |
| `STORAGE_PATH` | No | `./uploads` | Directory for uploaded files |
| `MAX_FILE_SIZE` | No | `104857600` (100 MiB) | Maximum upload size in bytes |
| `CHUNK_SIZE` | No | `65536` (64 KiB) | Chunk size for streaming |
| `RUST_LOG` | No | — | Tracing filter (e.g. `info`, `debug`) |

## Development

```bash
task test          # Run all tests
task lint          # Clippy with pedantic warnings
task fmt           # Format code
task pre:commit    # Format + lint + test
task bench         # Run criterion benchmarks
task tdd           # Watch mode: auto-run tests on save
task dev           # Watch mode: auto-rebuild and run server
task ci            # Full CI pipeline
```

### Project structure

```
src/
  lib.rs              # Proto module, re-exports
  config.rs           # Typed env config
  auth.rs             # Auth interceptor (constant-time, DI)
  interceptor.rs      # Request-ID interceptor (UUID v7)
  health.rs           # tonic-health service + background monitor
  rest.rs             # REST API (axum) with auth middleware
  service.rs          # FileUpload gRPC trait implementation
  storage/
    mod.rs            # Storage trait abstraction
    local.rs          # Local filesystem backend + stale upload reaper
  server/main.rs      # Server binary (gRPC + REST)
  client/main.rs      # Client binary
proto/
  file_upload.proto   # Service and message definitions
benches/
  file_upload.rs      # Criterion benchmarks
tests/
  integration.rs      # Full transport integration tests
```

## Tests

17 tests covering:

- Upload validation (empty stream, invalid first message)
- Upload/download round-trip with checksum verification
- Metadata retrieval and not-found errors
- Delete and not-found errors
- Auth interceptor (missing, invalid, valid tokens)
- REST auth middleware (missing, invalid, valid tokens)
- Request-ID interceptor (generation, propagation)
- Config parsing (missing values, invalid values, defaults)
- Full gRPC transport integration (upload → metadata → download → delete → verify gone)

## Benchmarks

Criterion benchmarks measure throughput across file sizes (1 KB – 1 MB):

```bash
task bench
```

Three benchmark groups:
- **upload** — end-to-end chunked upload
- **download** — upload then stream download
- **storage_append** — raw storage write throughput

## License

MIT License — see [LICENSE](LICENSE).
