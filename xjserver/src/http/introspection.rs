use axum::Json;
use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use serde::Serialize;

use crate::config::HealthIntrospectionConfig;
use crate::http::HttpState;

const EXPLORER_JS: &[u8] = include_bytes!("../../explorer/dist/xj-explorer.js");

#[derive(Serialize)]
struct HealthResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    checks: Option<Vec<HealthCheckResponse>>,
}

#[derive(Serialize)]
struct HealthCheckResponse {
    name: String,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

pub async fn health_handler(State(state): State<HttpState>) -> Response {
    let Some(introspection) = state.config.introspection.as_ref() else {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    };
    let Some(health) = introspection.health.as_ref() else {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    };

    health_response(health).await
}

pub async fn manifest_handler(State(state): State<HttpState>) -> Response {
    let Some(introspection) = state.config.introspection.as_ref() else {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    };
    if introspection.manifest.is_none() {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }

    Json(state.manifest.as_ref().clone()).into_response()
}

pub async fn explorer_page_handler(State(state): State<HttpState>) -> Response {
    let Some(introspection) = state.config.introspection.as_ref() else {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    };
    let Some(explorer) = introspection.explorer.as_ref() else {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    };
    let Some(manifest) = introspection.manifest.as_ref() else {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    };

    let script_src = format!(
        "{}/xj-explorer.js",
        explorer.assets_path.trim_end_matches('/')
    );
    let html = render_explorer_page(&script_src, &manifest.path);
    Html(html).into_response()
}

pub async fn explorer_asset_handler(State(state): State<HttpState>) -> Response {
    let Some(introspection) = state.config.introspection.as_ref() else {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    };
    if introspection.explorer.is_none() {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }

    let mut response = EXPLORER_JS.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/javascript; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400"),
    );
    response
}

pub fn render_explorer_page(script_src: &str, manifest_url: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>XJ Explorer</title>
</head>
<body>
  <div id="xj-explorer-root" data-manifest-url="{}"></div>
  <script src="{}"></script>
</body>
</html>
"#,
        escape_html_attribute(manifest_url),
        escape_html_attribute(script_src)
    )
}

fn escape_html_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

async fn health_response(health: &HealthIntrospectionConfig) -> Response {
    let mut checks = Vec::new();

    for check in &health.checks {
        let result = check.run().await;
        checks.push(HealthCheckResponse {
            name: result.name,
            ok: result.ok,
            message: result.message,
        });
    }

    let ok = checks.is_empty() || checks.iter().all(|c| c.ok);
    let status = if ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    let body = HealthResponse {
        ok,
        checks: if checks.is_empty() {
            None
        } else {
            Some(checks)
        },
    };

    (status, Json(body)).into_response()
}
