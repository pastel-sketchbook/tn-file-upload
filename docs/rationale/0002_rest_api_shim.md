# 0002: REST API Shim for Browser SPA

## Status

Accepted

## Context

The gRPC file upload service uses client-streaming for uploads and server-streaming for downloads. A browser-based SPA needs to interact with this service, but faces fundamental protocol constraints:

1. **gRPC-Web does not support client-streaming.** The gRPC-Web specification only supports unary and server-streaming RPCs. Our `Upload` RPC is client-streaming — the browser cannot call it directly.

2. **HTTP/2 framing requirements.** Native gRPC requires HTTP/2 with trailers, which browsers do not expose through the Fetch API. gRPC-Web works around this with a custom wire format, but the client-streaming limitation remains.

3. **Connect protocol (Buf)** supports client-streaming via HTTP/2, but browser environments are limited to HTTP/1.1 semantics through `fetch()`. Only server-streaming works reliably in browsers via the Connect protocol.

## Decision

Add a thin REST API layer (axum) to the Rust server that runs on a separate port alongside gRPC. The REST endpoints wrap the same `Storage` trait used by the gRPC handlers:

| Endpoint | Method | Maps to |
|---|---|---|
| `/api/upload` | POST (multipart) | `Storage::create` + `append` + `finalize` |
| `/api/files` | GET | (list — pending trait addition) |
| `/api/files/{id}` | GET | `Storage::metadata` |
| `/api/files/{id}` | DELETE | `Storage::delete` |
| `/api/files/{id}/download` | GET | `Storage::read_chunks` |

The SPA talks REST over HTTP/1.1. The gRPC interface remains the primary API for service-to-service communication, CLI tools, and clients that support HTTP/2.

## Alternatives Considered

### 1. gRPC-Web with unary upload

Redesign the upload RPC as unary (send entire file in one message). Rejected because:
- Defeats the purpose of streaming — requires buffering the entire file in memory on both client and server.
- Protobuf message size limits (default 4 MB) would require raising limits or splitting into multiple unary calls with server-side reassembly — essentially reinventing chunked upload over unary RPCs.

### 2. Envoy/nginx gRPC-Web proxy

Deploy a sidecar proxy that translates gRPC-Web to native gRPC. Rejected because:
- Adds deployment complexity for a development/demo tool.
- Still doesn't solve client-streaming — Envoy's gRPC-Web filter has the same limitation.

### 3. tonic-web (built-in gRPC-Web support)

Use `tonic-web` to serve gRPC-Web directly from the Rust server. Rejected because:
- Same client-streaming limitation — the browser still cannot stream upload chunks.
- Would work for `GetMetadata`, `Delete`, and `Download` (server-streaming), but not `Upload`.

### 4. Separate REST gateway service

Build a standalone HTTP service that calls gRPC internally. Rejected because:
- Over-engineered for a single binary deployment.
- The storage layer is already abstracted behind a trait — calling it directly from axum handlers is simpler and avoids the gRPC round-trip overhead.

### 5. Plain Vite SPA with no server-side rendering

This is what we chose for the web app itself (over TanStack Start SSR). The SPA makes direct REST calls to the Rust server. In development, Vite's proxy forwards `/api/*` to the REST port. In production, the SPA would be served as static files with the REST API on the same origin or behind a reverse proxy.

## Consequences

**Positive:**
- Browser clients work with standard `fetch()` and `FormData` — no special client libraries needed.
- Upload uses `multipart/form-data` which browsers handle natively, including progress events via `XMLHttpRequest` if needed later.
- The REST layer is ~200 lines and shares the same `Arc<dyn Storage>` — no data duplication or synchronization concerns.
- gRPC remains the canonical API; REST is a convenience shim.

**Negative:**
- Two ports to manage (gRPC :50051, REST :3001). A future improvement could multiplex both on one port using content-type sniffing or path-based routing.
- The REST API does not have authentication parity with gRPC (no `x-auth-token` interceptor). This is acceptable for local development but must be addressed before production exposure.
- Upload is not truly streaming on the REST side — axum's `Multipart` extractor buffers the field body. For very large files, this increases memory usage compared to the gRPC path. Mitigation: axum supports streaming multipart fields via `Field::chunk()` — we can switch to incremental reading in a future iteration.

## References

- [gRPC-Web protocol spec — streaming limitations](https://github.com/grpc/grpc-web/blob/master/doc/streaming-roadmap.md)
- [Connect protocol — browser constraints](https://connectrpc.com/docs/protocol/#streaming-rpcs)
- [axum multipart extractor](https://docs.rs/axum/latest/axum/extract/struct.Multipart.html)
