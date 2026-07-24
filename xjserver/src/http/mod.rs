use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use http::Extensions;

use crate::config::XJConfig;
use crate::context::ContextBase;
use crate::error::XJError;
use crate::login_token_middleware::LoginTokenMiddleware;
use crate::manifest::{XJManifest, build_manifest};
use crate::metadata::Metadata;
use crate::registry::{RouteBucket, RouteRegistry};
use crate::session::Session;
use crate::session_middleware::SessionMiddleware;

mod cors;
pub(crate) mod introspection;
mod rate_limit;
mod response;
mod security;

pub use rate_limit::RateLimiters;

use cors::cors_layer;
use introspection::{
    explorer_asset_handler, explorer_page_handler, health_handler, manifest_handler,
};
use rate_limit::{global_rate_limit, login_rate_limit, xj_rate_limit, XjRateLimitState};
use security::security_headers;

#[derive(Clone)]
pub struct HttpState {
    pub registry: Arc<RouteRegistry>,
    pub config: Arc<XJConfig>,
    pub app_state: Arc<dyn Any + Send + Sync>,
    pub session_middleware: Arc<dyn SessionMiddleware>,
    pub login_token_middleware: Arc<dyn LoginTokenMiddleware>,
    pub manifest: Arc<XJManifest>,
    pub rate_limiters: RateLimiters,
}

impl HttpState {
    pub fn new(
        registry: RouteRegistry,
        config: XJConfig,
        app_state: Arc<dyn Any + Send + Sync>,
        session_middleware: Arc<dyn SessionMiddleware>,
        login_token_middleware: Arc<dyn LoginTokenMiddleware>,
        http_port: Option<u16>,
    ) -> Result<Self, String> {
        if let Some(introspection) = &config.introspection {
            introspection.validate()?;
        }
        let manifest = Arc::new(build_manifest(&registry, &config, http_port));
        Ok(Self {
            registry: Arc::new(registry),
            config: Arc::new(config),
            app_state,
            session_middleware,
            login_token_middleware,
            manifest,
            rate_limiters: RateLimiters::new(),
        })
    }

    async fn resolve_session(&self, headers: &HeaderMap) -> Result<Session, XJError> {
        let metadata = Metadata::from_header_iter(headers.iter().filter_map(|(k, v)| {
            let value = v.to_str().ok()?;
            Some((k.as_str(), value))
        }));
        self.session_middleware
            .resolve(&metadata, &self.config)
            .await
    }
}

/// XJ-RPC HTTP router with middleware stack (§3 PLAN).
pub fn xj_router(state: HttpState) -> Router {
    let body_limit = state.config.body_limit;

    let xj_rate_state = XjRateLimitState {
        limiter: state.rate_limiters.xj.clone(),
        session_middleware: state.session_middleware.clone(),
        config: state.config.clone(),
    };

    let login_router = Router::new()
        .route("/{name}", post(dispatch_login))
        .route_layer(middleware::from_fn_with_state(
            state.rate_limiters.login.clone(),
            login_rate_limit,
        ))
        .with_state(state.clone());

    let xj_router = Router::new()
        .route("/{name}", post(dispatch_xj))
        .route_layer(middleware::from_fn_with_state(
            xj_rate_state,
            xj_rate_limit,
        ))
        .with_state(state.clone());

    let rpc = Router::new()
        .nest("/login", login_router)
        .nest("/xj", xj_router)
        .route("/session/{name}", post(dispatch_session))
        .route("/logout/{name}", post(dispatch_logout))
        .with_state(state.clone());

    let router = if let Some(introspection) = &state.config.introspection {
        let mut introspection_router = Router::new().with_state(state.clone());

        if introspection.health.is_some() {
            let path = introspection
                .health
                .as_ref()
                .map(|h| h.path.clone())
                .unwrap_or_else(|| "/health".into());
            introspection_router = introspection_router.route(&path, get(health_handler));
        }

        if introspection.manifest.is_some() {
            let path = introspection
                .manifest
                .as_ref()
                .map(|m| m.path.clone())
                .unwrap_or_else(|| "/__xj/manifest".into());
            introspection_router = introspection_router.route(&path, get(manifest_handler));
        }

        if let Some(explorer) = &introspection.explorer {
            let docs_path = explorer.path.clone();
            let asset_path = format!(
                "{}/xj-explorer.js",
                explorer.assets_path.trim_end_matches('/')
            );
            introspection_router = introspection_router
                .route(&docs_path, get(explorer_page_handler))
                .route(&asset_path, get(explorer_asset_handler));
        }

        Router::new()
            .merge(introspection_router)
            .merge(rpc)
    } else {
        Router::new().merge(rpc)
    };

    let mut router = router
        .layer(middleware::from_fn_with_state(
            state.rate_limiters.global.clone(),
            global_rate_limit,
        ))
        .layer(DefaultBodyLimit::max(body_limit))
        .layer(middleware::from_fn(security_headers))
        .with_state(state.clone());

    if let Some(cors) = cors_layer(&state.config) {
        router = router.layer(cors);
    }

    router
}

async fn dispatch_xj(
    State(state): State<HttpState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    dispatch_bucket(&state, RouteBucket::Xj, &name, headers, body, false).await
}

async fn dispatch_login(
    State(state): State<HttpState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    dispatch_bucket(&state, RouteBucket::Login, &name, headers, body, true).await
}

async fn dispatch_session(
    State(state): State<HttpState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    dispatch_bucket(&state, RouteBucket::Session, &name, headers, body, false).await
}

async fn dispatch_logout(
    State(state): State<HttpState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    dispatch_bucket(&state, RouteBucket::Logout, &name, headers, body, false).await
}

async fn dispatch_bucket(
    state: &HttpState,
    bucket: RouteBucket,
    name: &str,
    headers: HeaderMap,
    body: Bytes,
    issue_login_token: bool,
) -> Response {
    let session = match state.resolve_session(&headers).await {
        Ok(session) => session,
        Err(err) => return err.into_response(),
    };

    let Some(route) = state.registry.get(bucket, name) else {
        let label = match bucket {
            RouteBucket::Xj => "Route",
            RouteBucket::Login => "Login route",
            RouteBucket::Session => "Session route",
            RouteBucket::Logout => "Logout route",
        };
        return XJError::not_found(format!("{label} not found: {name}")).into_response();
    };

    let metadata = Metadata::from_header_iter(headers.iter().filter_map(|(k, v)| {
        let value = v.to_str().ok()?;
        Some((k.as_str(), value))
    }));

    let base = ContextBase {
        session,
        metadata,
        state: state.app_state.clone(),
        config: state.config.clone(),
        extensions: Extensions::new(),
    };

    match route.dispatch(&body, base).await {
        Ok(outcome) => {
            let mut response = Json(outcome.body.clone()).into_response();
            apply_outgoing_headers(response.headers_mut(), outcome.metadata.outgoing());

            if issue_login_token {
                match state
                    .login_token_middleware
                    .issue(&outcome.body, &outcome.metadata, &state.config, name)
                    .await
                {
                    Ok(Some(token)) => {
                        let header_name = &state.config.token_header;
                        let value = format_token_header_value(&token, &state.config.token_prefix);
                        if let (Ok(name), Ok(value)) = (
                            HeaderName::from_bytes(header_name.as_bytes()),
                            HeaderValue::from_str(&value),
                        ) {
                            response.headers_mut().insert(name, value);
                        }
                    }
                    Ok(None) => {}
                    Err(err) => return err.into_response(),
                }
            }

            response
        }
        Err(err) => err.into_response(),
    }
}

fn format_token_header_value(token: &str, prefix: &str) -> String {
    if prefix.is_empty() {
        token.to_string()
    } else {
        format!("{prefix} {token}")
    }
}

fn apply_outgoing_headers(headers: &mut HeaderMap, outgoing: &HashMap<String, String>) {
    for (name, value) in outgoing {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            headers.insert(name, value);
        }
    }
}
