use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{ConnectInfo, Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use governor::{
    Quota, RateLimiter,
    clock::DefaultClock,
    state::keyed::DefaultKeyedStateStore,
};

use crate::config::XJConfig;
use crate::error::XJError;
use crate::metadata::Metadata;
use crate::session_middleware::SessionMiddleware;

pub type KeyedLimiter = RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>;

#[derive(Clone)]
pub struct RateLimiters {
    pub global: Arc<KeyedLimiter>,
    pub login: Arc<KeyedLimiter>,
    pub xj: Arc<KeyedLimiter>,
}

impl RateLimiters {
    pub fn new() -> Self {
        Self {
            global: Arc::new(create_limiter(
                "RATE_LIMIT_GLOBAL_WINDOW_MS",
                15 * 60 * 1000,
                "RATE_LIMIT_GLOBAL_MAX",
                2000,
            )),
            login: Arc::new(create_limiter(
                "RATE_LIMIT_LOGIN_WINDOW_MS",
                15 * 60 * 1000,
                "RATE_LIMIT_LOGIN_MAX",
                20,
            )),
            xj: Arc::new(create_limiter(
                "RATE_LIMIT_XJ_WINDOW_MS",
                15 * 60 * 1000,
                "RATE_LIMIT_XJ_MAX",
                500,
            )),
        }
    }
}

#[derive(Clone)]
pub struct XjRateLimitState {
    pub limiter: Arc<KeyedLimiter>,
    pub session_middleware: Arc<dyn SessionMiddleware>,
    pub config: Arc<XJConfig>,
}

fn create_limiter(window_env: &str, window_default: u64, max_env: &str, max_default: u64) -> KeyedLimiter {
    let window_ms = parse_positive_env(window_env, window_default);
    let max = parse_positive_env(max_env, max_default);
    let max = NonZeroU32::new(max as u32).unwrap_or(NonZeroU32::new(1).unwrap());
    let period = Duration::from_millis(window_ms / u64::from(max.get()).max(1));
    let quota = Quota::with_period(period)
        .unwrap_or_else(|| Quota::per_second(NonZeroU32::new(1).unwrap()))
        .allow_burst(max);
    RateLimiter::keyed(quota)
}

fn parse_positive_env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(default)
}

pub fn client_ip(request: &Request<axum::body::Body>) -> String {
    request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

pub async fn global_rate_limit(
    State(limiter): State<Arc<KeyedLimiter>>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let key = client_ip(&request);
    match limiter.check_key(&key) {
        Ok(()) => next.run(request).await,
        Err(_) => XJError::TooManyRequests.into_response(),
    }
}

pub async fn login_rate_limit(
    State(limiter): State<Arc<KeyedLimiter>>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let key = client_ip(&request);
    match limiter.check_key(&key) {
        Ok(()) => next.run(request).await,
        Err(_) => XJError::TooManyRequests.into_response(),
    }
}

use crate::session::Session;

/// Rate-limit key for `/xj` routes (paridad Node `createXjRateLimiter`).
/// Guest / unresolved → `ip:{ip}`; authenticated → `u:{id}`.
pub(crate) fn xj_rate_limit_key(session: &Session, ip: &str) -> String {
    if session.is_guest() {
        format!("ip:{ip}")
    } else {
        format!("u:{}", session.id)
    }
}

pub async fn xj_rate_limit(
    State(state): State<XjRateLimitState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let metadata = Metadata::from_header_iter(request.headers().iter().filter_map(|(k, v)| {
        let value = v.to_str().ok()?;
        Some((k.as_str(), value))
    }));

    let ip = client_ip(&request);
    let key = match state
        .session_middleware
        .resolve(&metadata, &state.config)
        .await
    {
        Ok(session) => xj_rate_limit_key(&session, &ip),
        Err(_) => format!("ip:{ip}"),
    };

    match state.limiter.check_key(&key) {
        Ok(()) => next.run(request).await,
        Err(_) => XJError::TooManyRequests.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn xj_key_guest_uses_ip() {
        assert_eq!(xj_rate_limit_key(&Session::guest(), "127.0.0.1"), "ip:127.0.0.1");
    }

    #[test]
    fn xj_key_auth_uses_user_id() {
        let session = Session::from_claims(json!({"name": "ada", "id": 42})).unwrap();
        assert_eq!(xj_rate_limit_key(&session, "10.0.0.1"), "u:42");
    }
}
