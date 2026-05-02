use subtle::ConstantTimeEq;
use tonic::{Request, Status};

/// Creates an auth interceptor that validates the `x-auth-token` metadata
/// header against the provided expected token using constant-time comparison.
pub fn make_auth_interceptor(
    expected_token: String,
) -> impl Fn(Request<()>) -> Result<Request<()>, Status> + Clone + Send + Sync + 'static {
    move |req: Request<()>| {
        let token = req
            .metadata()
            .get("x-auth-token")
            .ok_or_else(|| Status::unauthenticated("missing auth token"))?;

        let token_str = token
            .to_str()
            .map_err(|_| Status::unauthenticated("invalid token encoding"))?;

        // Constant-time comparison to prevent timing attacks
        if token_str.as_bytes().ct_eq(expected_token.as_bytes()).into() {
            Ok(req)
        } else {
            Err(Status::unauthenticated("invalid auth token"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_token() {
        let interceptor = make_auth_interceptor("secret".into());
        let req = Request::new(());
        let err = interceptor(req).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
        assert!(err.message().contains("missing"));
    }

    #[test]
    fn rejects_invalid_token() {
        let interceptor = make_auth_interceptor("secret".into());
        let mut req = Request::new(());
        req.metadata_mut()
            .insert("x-auth-token", "wrong".parse().unwrap());
        let err = interceptor(req).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn accepts_valid_token() {
        let interceptor = make_auth_interceptor("secret".into());
        let mut req = Request::new(());
        req.metadata_mut()
            .insert("x-auth-token", "secret".parse().unwrap());
        assert!(interceptor(req).is_ok());
    }
}
