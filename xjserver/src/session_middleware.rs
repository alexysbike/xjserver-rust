//! Session resolution from request metadata (JWT by default).

use async_trait::async_trait;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, errors::ErrorKind};
use serde_json::Value;

use crate::config::XJConfig;
use crate::error::XJError;
use crate::metadata::Metadata;
use crate::session::Session;

/// Resolves a [`Session`] from incoming headers / config.
#[async_trait]
pub trait SessionMiddleware: Send + Sync {
    async fn resolve(&self, metadata: &Metadata, config: &XJConfig) -> Result<Session, XJError>;
}

/// Default JWT session middleware (paridad Node `XJJWTSessionMiddleware`).
#[derive(Debug, Default, Clone, Copy)]
pub struct JwtSessionMiddleware;

#[async_trait]
impl SessionMiddleware for JwtSessionMiddleware {
    async fn resolve(&self, metadata: &Metadata, config: &XJConfig) -> Result<Session, XJError> {
        let header_name = config.token_header.to_ascii_lowercase();
        let Some(auth_header) = metadata.get_incoming(&header_name) else {
            return Ok(Session::guest());
        };

        let prefix = format!("{} ", config.token_prefix);
        if !auth_header.starts_with(&prefix) {
            return Ok(Session::guest());
        }

        let token = &auth_header[prefix.len()..];
        let Some(secret) = config.jwt_secret.as_deref() else {
            return Ok(Session::guest());
        };

        let mut validation = Validation::new(Algorithm::HS256);
        // Login result may carry arbitrary claims; we only require a valid signature + exp.
        validation.validate_aud = false;
        validation.required_spec_claims.clear();

        let token_data = decode::<Value>(
            token,
            &DecodingKey::from_secret(secret.as_bytes()),
            &validation,
        )
        .map_err(|err| match err.kind() {
            ErrorKind::ExpiredSignature => XJError::forbidden("Token expired"),
            ErrorKind::InvalidToken
            | ErrorKind::InvalidSignature
            | ErrorKind::InvalidAlgorithm
            | ErrorKind::Base64(_)
            | ErrorKind::Utf8(_) => XJError::bad_request("Invalid token"),
            _ => XJError::bad_request("Token verification failed"),
        })?;

        Session::from_claims(token_data.claims)
    }
}
