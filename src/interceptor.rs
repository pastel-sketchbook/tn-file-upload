use tonic::{Request, Status};
use uuid::Uuid;

/// Newtype for the request-scoped trace ID.
#[derive(Clone, Debug)]
pub struct RequestId(pub String);

/// Injects a request-scoped trace ID (UUID v7) into request extensions.
///
/// Propagates an inbound `x-request-id` header if present, otherwise generates a new UUID v7.
///
/// # Errors
///
/// This interceptor does not fail.
pub fn request_id_interceptor(mut req: Request<()>) -> Result<Request<()>, Status> {
    let request_id = req
        .metadata()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map_or_else(|| Uuid::now_v7().to_string(), String::from);

    tracing::Span::current().record("request_id", request_id.as_str());
    req.extensions_mut().insert(RequestId(request_id));

    Ok(req)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_request_id_when_missing() {
        let req = Request::new(());
        let result = request_id_interceptor(req).unwrap();
        let id = result.extensions().get::<RequestId>().unwrap();
        assert!(!id.0.is_empty());
        // UUID v7 format
        assert_eq!(id.0.len(), 36);
    }

    #[test]
    fn propagates_inbound_request_id() {
        let mut req = Request::new(());
        req.metadata_mut()
            .insert("x-request-id", "my-trace-123".parse().unwrap());
        let result = request_id_interceptor(req).unwrap();
        let id = result.extensions().get::<RequestId>().unwrap();
        assert_eq!(id.0, "my-trace-123");
    }
}
