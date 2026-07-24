//! Login token issuance (JWT by default).

use async_trait::async_trait;
use chrono::{Duration, Utc};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde_json::{Map, Value};

use crate::config::XJConfig;
use crate::error::XJError;
use crate::metadata::Metadata;

/// Issues an auth token after a successful login route.
#[async_trait]
pub trait LoginTokenMiddleware: Send + Sync {
    async fn issue(
        &self,
        result: &Value,
        metadata: &Metadata,
        config: &XJConfig,
        login_name: &str,
    ) -> Result<Option<String>, XJError>;
}

/// Default JWT login-token middleware (paridad Node `XJJWTLoginTokenMiddleware`).
#[derive(Debug, Default, Clone, Copy)]
pub struct JwtLoginTokenMiddleware;

#[async_trait]
impl LoginTokenMiddleware for JwtLoginTokenMiddleware {
    async fn issue(
        &self,
        result: &Value,
        _metadata: &Metadata,
        config: &XJConfig,
        _login_name: &str,
    ) -> Result<Option<String>, XJError> {
        let Some(secret) = config.jwt_secret.as_deref() else {
            return Ok(None);
        };

        let mut claims = match result {
            Value::Object(map) => map.clone(),
            _ => {
                let mut map = Map::new();
                map.insert("data".to_string(), result.clone());
                map
            }
        };

        let expires_in = parse_expires_in(&config.jwt_expires_in).map_err(|msg| {
            XJError::internal(format!("Invalid jwt_expires_in '{}': {msg}", config.jwt_expires_in))
        })?;
        let exp = (Utc::now() + expires_in).timestamp();
        claims.insert("exp".to_string(), Value::from(exp));
        claims.insert("iat".to_string(), Value::from(Utc::now().timestamp()));

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .map_err(|err| XJError::internal(format!("Failed to sign JWT: {err}")))?;

        Ok(Some(token))
    }
}

/// Parse durations like `"10h"`, `"15m"`, `"30s"`, `"1d"`, or bare seconds `"3600"`.
fn parse_expires_in(raw: &str) -> Result<Duration, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("empty duration".into());
    }

    if let Ok(secs) = raw.parse::<i64>() {
        return Ok(Duration::seconds(secs));
    }

    let (num_str, unit) = raw.split_at(raw.len().saturating_sub(1));
    let n: i64 = num_str
        .parse()
        .map_err(|_| format!("expected number with unit, got '{raw}'"))?;

    match unit {
        "s" => Ok(Duration::seconds(n)),
        "m" => Ok(Duration::minutes(n)),
        "h" => Ok(Duration::hours(n)),
        "d" => Ok(Duration::days(n)),
        _ => Err(format!("unknown unit '{unit}' in '{raw}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_expires_in;
    use chrono::Duration;

    #[test]
    fn parses_common_units() {
        assert_eq!(parse_expires_in("10h").unwrap(), Duration::hours(10));
        assert_eq!(parse_expires_in("15m").unwrap(), Duration::minutes(15));
        assert_eq!(parse_expires_in("30s").unwrap(), Duration::seconds(30));
        assert_eq!(parse_expires_in("1d").unwrap(), Duration::days(1));
        assert_eq!(parse_expires_in("3600").unwrap(), Duration::seconds(3600));
    }
}
