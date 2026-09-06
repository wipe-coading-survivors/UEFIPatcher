use tonic::{Request, Status};

#[allow(clippy::result_large_err)]
pub fn check_auth<T>(req: &Request<T>, sm: &crate::session::SessionManager) -> Result<(), Status> {
    let auth = req
        .metadata()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Status::unauthenticated("missing Authorization"))?;
    let token = auth
        .strip_prefix("Bearer ")
        .ok_or_else(|| Status::unauthenticated("bad Bearer"))?;
    let session_id = req
        .metadata()
        .get("x-session-id")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Status::unauthenticated("missing session-id"))?;
    if !sm.validate_token(session_id, token) {
        return Err(Status::unauthenticated("invalid token"));
    }
    Ok(())
}
