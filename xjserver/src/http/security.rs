use axum::http::{HeaderValue, Request, header};
use axum::middleware::Next;
use axum::response::Response;

/// Security headers equivalent to helmet defaults (subset).
pub async fn security_headers(request: Request<axum::body::Body>, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::X_FRAME_OPTIONS,
        HeaderValue::from_static("DENY"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::HeaderName::from_static("x-dns-prefetch-control"),
        HeaderValue::from_static("off"),
    );

    response
}
