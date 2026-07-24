use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;

/// Result of a single health check.
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    pub name: String,
    pub ok: bool,
    pub message: Option<String>,
}

/// Custom health check (async).
#[async_trait]
pub trait HealthCheck: Send + Sync {
    async fn run(&self) -> HealthCheckResult;
}

/// `{ apiBaseUrl }` section in the manifest.
#[derive(Debug, Clone, Default)]
pub struct ManifestClientConfig {
    pub api_base_url: Option<String>,
}

impl ManifestClientConfig {
    pub fn api_base_url(mut self, url: impl Into<String>) -> Self {
        self.api_base_url = Some(url.into());
        self
    }
}

/// CORS setting for the HTTP adapter (paridad Node `http.cors`).
#[derive(Debug, Clone)]
pub enum HttpCorsConfig {
    /// `cors: false` — no CORS middleware.
    Disabled,
    /// `cors: true` — permissive (reflect origin).
    Permissive,
    /// `cors: undefined` — legacy: only expose token header.
    Default,
    /// Explicit CORS options object.
    Custom(CustomCorsConfig),
}

#[derive(Debug, Clone)]
pub struct CustomCorsConfig {
    pub allow_origin: bool,
    pub methods: Option<Vec<String>>,
    pub allowed_headers: Option<Vec<String>>,
    pub exposed_headers: Option<Vec<String>>,
    pub credentials: bool,
    pub max_age: Option<u64>,
}

impl Default for CustomCorsConfig {
    fn default() -> Self {
        Self {
            allow_origin: true,
            methods: None,
            allowed_headers: None,
            exposed_headers: None,
            credentials: false,
            max_age: None,
        }
    }
}

/// HTTP transport config surface.
#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub port: Option<u16>,
    pub cors: HttpCorsConfig,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            port: None,
            cors: HttpCorsConfig::Default,
        }
    }
}

impl HttpConfig {
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    pub fn cors(mut self, cors: HttpCorsConfig) -> Self {
        self.cors = cors;
        self
    }
}

/// Health introspection config (`introspection.health`).
#[derive(Clone)]
pub struct HealthIntrospectionConfig {
    pub path: String,
    pub checks: Vec<Arc<dyn HealthCheck>>,
}

impl Default for HealthIntrospectionConfig {
    fn default() -> Self {
        Self {
            path: "/health".into(),
            checks: Vec::new(),
        }
    }
}

impl HealthIntrospectionConfig {
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    pub fn check(mut self, check: Arc<dyn HealthCheck>) -> Self {
        self.checks.push(check);
        self
    }
}

/// Manifest introspection config (`introspection.manifest`).
#[derive(Debug, Clone)]
pub struct ManifestIntrospectionConfig {
    pub path: String,
}

impl Default for ManifestIntrospectionConfig {
    fn default() -> Self {
        Self {
            path: "/__xj/manifest".into(),
        }
    }
}

impl ManifestIntrospectionConfig {
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }
}

/// Explorer UI config (`introspection.explorer`). Requires manifest enabled.
#[derive(Debug, Clone)]
pub struct ExplorerIntrospectionConfig {
    /// HTML shell path (default `/__xj/docs`).
    pub path: String,
    /// Static assets mount path (default `/__xj/explorer`).
    pub assets_path: String,
}

impl Default for ExplorerIntrospectionConfig {
    fn default() -> Self {
        Self {
            path: "/__xj/docs".into(),
            assets_path: "/__xj/explorer".into(),
        }
    }
}

impl ExplorerIntrospectionConfig {
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    pub fn assets_path(mut self, path: impl Into<String>) -> Self {
        self.assets_path = path.into();
        self
    }
}

/// Introspection endpoints (health + manifest + explorer embedded in HTTP server).
/// When omitted from [`XJConfig`], no introspection routes are mounted.
#[derive(Clone)]
pub struct IntrospectionConfig {
    pub port: Option<u16>,
    pub health: Option<HealthIntrospectionConfig>,
    pub manifest: Option<ManifestIntrospectionConfig>,
    pub explorer: Option<ExplorerIntrospectionConfig>,
}

impl IntrospectionConfig {
    /// Health + manifest + explorer enabled with default paths.
    pub fn enabled() -> Self {
        Self {
            port: None,
            health: Some(HealthIntrospectionConfig::default()),
            manifest: Some(ManifestIntrospectionConfig::default()),
            explorer: Some(ExplorerIntrospectionConfig::default()),
        }
    }

    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    pub fn health(mut self, health: HealthIntrospectionConfig) -> Self {
        self.health = Some(health);
        self
    }

    pub fn manifest(mut self, manifest: ManifestIntrospectionConfig) -> Self {
        self.manifest = Some(manifest);
        self
    }

    pub fn explorer(mut self, explorer: ExplorerIntrospectionConfig) -> Self {
        self.explorer = Some(explorer);
        self
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.health.is_none() && self.manifest.is_none() {
            return Err(
                "[XJServer] introspection must enable at least one of health or manifest"
                    .into(),
            );
        }
        if self.explorer.is_some() && self.manifest.is_none() {
            return Err(
                "[XJServer] introspection.explorer requires introspection.manifest"
                    .into(),
            );
        }
        Ok(())
    }
}

/// gRPC transport config (paridad Node `GrpcTransportConfig`).
#[derive(Debug, Clone)]
pub struct GrpcConfig {
    pub port: u16,
    /// Output path where the generated protobuf definition is written on serve.
    pub proto_path: PathBuf,
    /// Protobuf package name. Defaults to `xjserver.v1`.
    pub package: String,
    /// When false (default), clients map proto snake_case ↔ camelCase.
    pub keep_case: bool,
    pub max_receive_message_length: usize,
    pub max_send_message_length: usize,
}

impl GrpcConfig {
    pub fn new(port: u16, proto_path: impl Into<PathBuf>) -> Self {
        Self {
            port,
            proto_path: proto_path.into(),
            package: "xjserver.v1".into(),
            keep_case: false,
            max_receive_message_length: 50 * 1024 * 1024,
            max_send_message_length: 50 * 1024 * 1024,
        }
    }

    pub fn package(mut self, package: impl Into<String>) -> Self {
        self.package = package.into();
        self
    }

    pub fn keep_case(mut self, keep_case: bool) -> Self {
        self.keep_case = keep_case;
        self
    }

    pub fn max_receive_message_length(mut self, bytes: usize) -> Self {
        self.max_receive_message_length = bytes;
        self
    }

    pub fn max_send_message_length(mut self, bytes: usize) -> Self {
        self.max_send_message_length = bytes;
        self
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.proto_path.as_os_str().is_empty() {
            return Err("[XJServer] grpc.proto_path is required when grpc is configured".into());
        }
        Ok(())
    }
}

/// Server config (Phase 2+ surface).
///
/// Prefer fluent setters over `let mut` + nested field assignment:
///
/// ```ignore
/// let config = XJConfig::default()
///     .service_name("my-service")
///     .jwt_secret("dev-secret")
///     .http_with(|h| h.port(3005).cors(HttpCorsConfig::Default))
///     .introspection(IntrospectionConfig::enabled())
///     .grpc(GrpcConfig::new(50051, "target/service.proto"));
/// ```
#[derive(Clone)]
pub struct XJConfig {
    pub service_name: Option<String>,
    pub jwt_secret: Option<String>,
    /// JWT expiration string, e.g. `"10h"`, `"15m"`, `"30s"`.
    pub jwt_expires_in: String,
    pub token_header: String,
    pub token_prefix: String,
    /// Max request body size in bytes (default 50MB).
    pub body_limit: usize,
    pub manifest_client: Option<ManifestClientConfig>,
    pub http: HttpConfig,
    /// When set, gRPC transport is available via [`crate::XJServer::serve_grpc`].
    pub grpc: Option<GrpcConfig>,
    /// When `None`, introspection routes are not mounted.
    pub introspection: Option<IntrospectionConfig>,
}

impl Default for XJConfig {
    fn default() -> Self {
        Self {
            service_name: None,
            jwt_secret: None,
            jwt_expires_in: "10h".to_string(),
            token_header: "x-xj-token".to_string(),
            token_prefix: "Bearer".to_string(),
            body_limit: 50 * 1024 * 1024,
            manifest_client: None,
            http: HttpConfig::default(),
            grpc: None,
            introspection: None,
        }
    }
}

impl XJConfig {
    pub fn service_name(mut self, name: impl Into<String>) -> Self {
        self.service_name = Some(name.into());
        self
    }

    pub fn jwt_secret(mut self, secret: impl Into<String>) -> Self {
        self.jwt_secret = Some(secret.into());
        self
    }

    pub fn jwt_expires_in(mut self, expires: impl Into<String>) -> Self {
        self.jwt_expires_in = expires.into();
        self
    }

    pub fn token_header(mut self, header: impl Into<String>) -> Self {
        self.token_header = header.into();
        self
    }

    pub fn token_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.token_prefix = prefix.into();
        self
    }

    pub fn body_limit(mut self, bytes: usize) -> Self {
        self.body_limit = bytes;
        self
    }

    pub fn manifest_client(mut self, client: ManifestClientConfig) -> Self {
        self.manifest_client = Some(client);
        self
    }

    /// Replace the entire HTTP config.
    pub fn http(mut self, http: HttpConfig) -> Self {
        self.http = http;
        self
    }

    /// Transform the current HTTP config in place (keeps defaults you don't touch).
    pub fn http_with(mut self, f: impl FnOnce(HttpConfig) -> HttpConfig) -> Self {
        self.http = f(self.http);
        self
    }

    pub fn grpc(mut self, grpc: GrpcConfig) -> Self {
        self.grpc = Some(grpc);
        self
    }

    pub fn introspection(mut self, introspection: IntrospectionConfig) -> Self {
        self.introspection = Some(introspection);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fluent_sets_nested_without_mutation() {
        let config = XJConfig::default()
            .service_name("grpc-demo")
            .jwt_secret("secret")
            .http_with(|h| h.port(3006).cors(HttpCorsConfig::Permissive))
            .introspection(IntrospectionConfig::enabled())
            .grpc(GrpcConfig::new(50052, "target/xjserver-grpc-demo.proto").package("demo.v1"));

        assert_eq!(config.service_name.as_deref(), Some("grpc-demo"));
        assert_eq!(config.jwt_secret.as_deref(), Some("secret"));
        assert_eq!(config.jwt_expires_in, "10h");
        assert_eq!(config.http.port, Some(3006));
        assert!(matches!(config.http.cors, HttpCorsConfig::Permissive));
        assert!(config.introspection.is_some());
        let grpc = config.grpc.expect("grpc");
        assert_eq!(grpc.port, 50052);
        assert_eq!(grpc.package, "demo.v1");
        assert_eq!(
            grpc.proto_path.as_os_str(),
            "target/xjserver-grpc-demo.proto"
        );
    }

    #[test]
    fn http_replace_preserves_unrelated_defaults() {
        let config = XJConfig::default().http(HttpConfig::default().port(1));
        assert_eq!(config.http.port, Some(1));
        assert!(matches!(config.http.cors, HttpCorsConfig::Default));
        assert!(config.grpc.is_none());
        assert!(config.introspection.is_none());
    }

    #[test]
    fn introspection_enabled_passes_validate() {
        IntrospectionConfig::enabled().validate().unwrap();
    }
}
