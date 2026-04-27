use std::sync::LazyLock;
use tonic::{Request, Status};

static AUTH_TOKEN: LazyLock<String> = LazyLock::new(|| {
    std::env::var("AUTH_TOKEN").unwrap_or_else(|_| {
        #[cfg(debug_assertions)]
        return "dev-token".into();
        #[cfg(not(debug_assertions))]
        panic!("AUTH_TOKEN must be set in production");
    })
});

/// Validates `x-auth-token` metadata header.
///
/// # Errors
///
/// Returns `Unauthenticated` if the token is missing, malformed, or invalid.
pub fn auth_interceptor(req: Request<()>) -> Result<Request<()>, Status> {
    let token = req
        .metadata()
        .get("x-auth-token")
        .ok_or_else(|| Status::unauthenticated("missing auth token"))?;

    let token_str = token
        .to_str()
        .map_err(|_| Status::unauthenticated("invalid token encoding"))?;

    if token_str != AUTH_TOKEN.as_str() {
        return Err(Status::unauthenticated("invalid auth token"));
    }

    Ok(req)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_token() {
        let req = Request::new(());
        let err = auth_interceptor(req).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
        assert!(err.message().contains("missing"));
    }

    #[test]
    fn rejects_invalid_token() {
        let mut req = Request::new(());
        req.metadata_mut()
            .insert("x-auth-token", "wrong".parse().unwrap());
        let err = auth_interceptor(req).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn accepts_valid_token() {
        let mut req = Request::new(());
        req.metadata_mut()
            .insert("x-auth-token", AUTH_TOKEN.parse().unwrap());
        assert!(auth_interceptor(req).is_ok());
    }
}
