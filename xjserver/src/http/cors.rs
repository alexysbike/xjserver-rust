use axum::http::{HeaderName, Method};
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::config::{CustomCorsConfig, HttpCorsConfig, XJConfig};

pub fn cors_layer(config: &XJConfig) -> Option<CorsLayer> {
    let token_header = config.token_header.clone();

    match &config.http.cors {
        HttpCorsConfig::Disabled => None,
        HttpCorsConfig::Permissive => Some(
            CorsLayer::new()
                .allow_origin(AllowOrigin::mirror_request())
                .allow_methods([
                    Method::GET,
                    Method::HEAD,
                    Method::PUT,
                    Method::PATCH,
                    Method::POST,
                    Method::DELETE,
                ])
                .allow_headers([
                    axum::http::header::CONTENT_TYPE,
                    header_name(&token_header),
                    axum::http::header::AUTHORIZATION,
                ])
                .expose_headers([header_name(&token_header)]),
        ),
        HttpCorsConfig::Default => Some(
            CorsLayer::new().expose_headers([header_name(&token_header)]),
        ),
        HttpCorsConfig::Custom(custom) => Some(build_custom_cors(custom, &token_header)),
    }
}

fn build_custom_cors(custom: &CustomCorsConfig, token_header: &str) -> CorsLayer {
    let default_methods = [
        Method::GET,
        Method::HEAD,
        Method::PUT,
        Method::PATCH,
        Method::POST,
        Method::DELETE,
    ];

    let methods: Vec<Method> = custom
        .methods
        .as_ref()
        .map(|m| {
            m.iter()
                .filter_map(|s| s.parse().ok())
                .collect::<Vec<_>>()
        })
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| default_methods.to_vec());

    let default_allowed = vec![
        "content-type".to_string(),
        token_header.to_string(),
        "authorization".to_string(),
    ];
    let allowed_headers: Vec<HeaderName> = custom
        .allowed_headers
        .as_ref()
        .unwrap_or(&default_allowed)
        .iter()
        .map(|s| header_name(s))
        .collect();

    let default_exposed = vec![token_header.to_string()];
    let exposed_headers: Vec<HeaderName> = custom
        .exposed_headers
        .as_ref()
        .unwrap_or(&default_exposed)
        .iter()
        .map(|s| header_name(s))
        .collect();

    let mut layer = CorsLayer::new()
        .allow_methods(methods)
        .allow_headers(allowed_headers)
        .expose_headers(exposed_headers);

    layer = if custom.allow_origin {
        layer.allow_origin(AllowOrigin::mirror_request())
    } else {
        layer
    };

    if custom.credentials {
        layer = layer.allow_credentials(true);
    }

    if let Some(max_age) = custom.max_age {
        layer = layer.max_age(std::time::Duration::from_secs(max_age));
    }

    layer
}

fn header_name(name: &str) -> HeaderName {
    HeaderName::from_bytes(name.as_bytes()).unwrap_or(axum::http::header::CONTENT_TYPE)
}
