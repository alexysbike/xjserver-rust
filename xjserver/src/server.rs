use std::any::Any;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;

use crate::config::{GrpcConfig, HttpCorsConfig, IntrospectionConfig, XJConfig};
use crate::error::XJError;
use crate::http::{HttpState, xj_router};
use crate::login_token_middleware::{JwtLoginTokenMiddleware, LoginTokenMiddleware};
use crate::registry::RouteRegistry;
use crate::route::XJRoute;
use crate::session_middleware::{JwtSessionMiddleware, SessionMiddleware};

/// Builder for `XJServer`. Collects routes and config without failing at each step;
/// registration errors (e.g. duplicate names) are accumulated and only
/// returned when `.state(...)` is called, which is what actually
/// builds the `XJServer`.
///
/// With the `discover` feature (default), [`XJServer::builder`] enables implicit
/// discover: `.state(...)` loads routes from `inventory`. Opt-out:
/// [`Self::skip_discover`] or [`XJServer::builder_without_discover`].
///
/// **Do not mix** `.route(x)` with macros that have `register = true` (default)
/// for the same route: fails with a duplicate name.
pub struct XJServerBuilder {
    registry: RouteRegistry,
    config: XJConfig,
    errors: Vec<XJError>,
    session_middleware: Arc<dyn SessionMiddleware>,
    login_token_middleware: Arc<dyn LoginTokenMiddleware>,
    /// When true (and feature `discover`), consume `inventory` submits in `.state`.
    discover: bool,
}

impl XJServerBuilder {
    fn with_registry_and_defaults(registry: RouteRegistry, discover: bool) -> Self {
        Self {
            registry,
            config: XJConfig::default(),
            errors: Vec::new(),
            session_middleware: Arc::new(JwtSessionMiddleware),
            login_token_middleware: Arc::new(JwtLoginTokenMiddleware),
            discover,
        }
    }

    pub fn new() -> Self {
        Self::with_registry_and_defaults(RouteRegistry::new(), false)
    }

    /// Starts the builder from an already-built `RouteRegistry`
    /// (for example, if you built it separately for tests or to
    /// compose route modules). Does not run inventory discover.
    pub fn with_registry(registry: RouteRegistry) -> Self {
        Self::with_registry_and_defaults(registry, false)
    }

    /// Do not load routes from `inventory` in `.state(...)`.
    ///
    /// Useful after [`XJServer::builder`] (discover on by default) when you want
    /// fully manual registration in this process.
    pub fn skip_discover(mut self) -> Self {
        self.discover = false;
        self
    }

    pub fn config(mut self, config: XJConfig) -> Self {
        self.config = config;
        self
    }

    /// Shortcut: set `config.service_name`.
    pub fn service_name(mut self, name: impl Into<String>) -> Self {
        self.config = self.config.service_name(name);
        self
    }

    /// Shortcut: set `config.jwt_secret`.
    pub fn jwt_secret(mut self, secret: impl Into<String>) -> Self {
        self.config = self.config.jwt_secret(secret);
        self
    }

    /// Shortcut: set `config.jwt_expires_in`.
    pub fn jwt_expires_in(mut self, expires: impl Into<String>) -> Self {
        self.config = self.config.jwt_expires_in(expires);
        self
    }

    /// Shortcut: set `config.http.port`.
    pub fn http_port(mut self, port: u16) -> Self {
        self.config = self.config.http_with(|h| h.port(port));
        self
    }

    /// Shortcut: set `config.http.cors`.
    pub fn cors(mut self, cors: HttpCorsConfig) -> Self {
        self.config = self.config.http_with(|h| h.cors(cors));
        self
    }

    /// Shortcut: enable introspection (`config.introspection`).
    pub fn introspection(mut self, introspection: IntrospectionConfig) -> Self {
        self.config = self.config.introspection(introspection);
        self
    }

    /// Shortcut: enable gRPC (`config.grpc`) with port + proto output path.
    pub fn grpc(mut self, port: u16, proto_path: impl Into<PathBuf>) -> Self {
        self.config = self.config.grpc(GrpcConfig::new(port, proto_path));
        self
    }

    /// Override the default JWT session middleware.
    pub fn session_middleware(mut self, mw: Arc<dyn SessionMiddleware>) -> Self {
        self.session_middleware = mw;
        self
    }

    /// Override the default JWT login-token middleware.
    pub fn login_token_middleware(mut self, mw: Arc<dyn LoginTokenMiddleware>) -> Self {
        self.login_token_middleware = mw;
        self
    }

    /// Registers a route in the `xj` bucket. Does not fail here: on a name
    /// collision, the error is stored and reported only in `.state(...)`.
    pub fn route<R>(mut self, route: R) -> Self
    where
        R: XJRoute + 'static,
        R::In: Send + Sync,
        R::Out: Send + Sync,
    {
        if let Err(err) = self.registry.add_xj(route) {
            self.errors.push(err);
        }
        self
    }

    /// Registers a route in the `login` bucket.
    pub fn login<R>(mut self, route: R) -> Self
    where
        R: XJRoute + 'static,
        R::In: Send + Sync,
        R::Out: Send + Sync,
    {
        if let Err(err) = self.registry.add_login(route) {
            self.errors.push(err);
        }
        self
    }

    /// Registers a route in the `session` bucket.
    pub fn session<R>(mut self, route: R) -> Self
    where
        R: XJRoute + 'static,
        R::In: Send + Sync,
        R::Out: Send + Sync,
    {
        if let Err(err) = self.registry.add_session(route) {
            self.errors.push(err);
        }
        self
    }

    /// Registers a route in the `logout` bucket.
    pub fn logout<R>(mut self, route: R) -> Self
    where
        R: XJRoute + 'static,
        R::In: Send + Sync,
        R::Out: Send + Sync,
    {
        if let Err(err) = self.registry.add_logout(route) {
            self.errors.push(err);
        }
        self
    }

    /// Application-specific state. This is where the builder is
    /// consumed and accumulated errors are resolved.
    ///
    /// If discover is active (`discover` feature + no `skip_discover`),
    /// loads [`crate::RouteRegistration`] from inventory first.
    pub fn state<S>(mut self, app_state: S) -> Result<XJServer, XJError>
    where
        S: Send + Sync + 'static,
    {
        self.apply_discover();

        if let Some(err) = self.errors.into_iter().next() {
            return Err(err);
        }

        if let Some(introspection) = &self.config.introspection {
            if let Err(err) = introspection.validate() {
                return Err(XJError::internal(err));
            }
        }

        if let Some(grpc) = &self.config.grpc {
            if let Err(err) = grpc.validate() {
                return Err(XJError::internal(err));
            }
        }

        let app_state: Arc<dyn Any + Send + Sync> = Arc::new(app_state);
        Ok(XJServer {
            state: HttpState::new(
                self.registry,
                self.config,
                app_state,
                self.session_middleware,
                self.login_token_middleware,
                None,
            )
            .map_err(XJError::internal)?,
        })
    }

    fn apply_discover(&mut self) {
        #[cfg(feature = "discover")]
        if self.discover {
            for reg in inventory::iter::<crate::discover::RouteRegistration> {
                if let Err(err) = self.registry.add_erased(reg.bucket, (reg.factory)()) {
                    self.errors.push(err);
                }
            }
            self.discover = false;
        }
    }
}

impl Default for XJServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Fully built XJ server. Encapsulates transport details (HTTP via axum,
/// gRPC via dynamic tonic) so consumers do not touch the adapters.
#[derive(Clone)]
pub struct XJServer {
    state: HttpState,
}

impl XJServer {
    /// Builder with implicit discover when the `discover` feature is enabled.
    ///
    /// Routes with `#[xj_route]` / `#[xj_login]` / … (`register = true`) or
    /// `#[xj_register(…)]` are loaded in [`.state`](XJServerBuilder::state).
    ///
    /// Opt-out: [`Self::builder_without_discover`] o [`XJServerBuilder::skip_discover`].
    pub fn builder() -> XJServerBuilder {
        #[cfg(feature = "discover")]
        {
            XJServerBuilder::with_registry_and_defaults(RouteRegistry::new(), true)
        }
        #[cfg(not(feature = "discover"))]
        {
            XJServerBuilder::new()
        }
    }

    /// Like [`Self::builder`] but without consuming `inventory` (manual registration).
    pub fn builder_without_discover() -> XJServerBuilder {
        XJServerBuilder::new()
    }

    /// Returns the raw `axum::Router`, for mounting inside
    /// a larger router or combining with your own HTTP routes.
    pub fn into_router(self) -> Router {
        xj_router(self.state)
    }

    /// Binds the HTTP listener and serves. Named `serve_http` (instead of
    /// `serve`) on purpose: coexists with `serve_grpc` without ambiguity.
    pub async fn serve_http(self, addr: &str) -> Result<(), std::io::Error> {
        let http_port = parse_http_port(addr);
        let mut state = self.state;
        let generated = state.config.grpc.as_ref().and_then(|grpc| {
            crate::grpc::generate_proto_from_registry(
                &state.registry,
                crate::grpc::GenerateProtoOptions {
                    package_name: Some(grpc.package.clone()),
                },
            )
            .ok()
        });
        state.manifest = Arc::new(crate::manifest::build_manifest_with_grpc(
            &state.registry,
            &state.config,
            http_port,
            generated.as_ref(),
        ));

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(
            listener,
            xj_router(state).into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
    }

    /// Binds the gRPC listener: generates `.proto`, writes it to `grpc.proto_path`
    /// and serves dynamic RPCs with the same dispatcher as HTTP.
    pub async fn serve_grpc(self, addr: &str) -> Result<(), std::io::Error> {
        let grpc = self.state.config.grpc.clone().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "[XJServer] grpc config is required for serve_grpc",
            )
        })?;

        let socket: SocketAddr = if let Ok(parsed) = addr.parse() {
            parsed
        } else {
            format!("0.0.0.0:{}", grpc.port)
                .parse()
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidInput, err))?
        };

        crate::grpc::start_grpc_server(self.state, &grpc, socket).await
    }

    /// Starts configured transports (`http.port` and/or `grpc`) and blocks
    /// until they finish. Node parity: `XJServer.run()`.
    ///
    /// Requires at least one of `http.port` or `grpc`. If both are set, serves
    /// them in parallel.
    pub async fn run(self) -> Result<(), std::io::Error> {
        let http_port = self.state.config.http.port;
        let grpc_port = self.state.config.grpc.as_ref().map(|g| g.port);

        if http_port.is_none() && grpc_port.is_none() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "[XJServer] At least one of http.port or grpc must be configured",
            ));
        }

        match (http_port, grpc_port) {
            (Some(http_port), Some(grpc_port)) => {
                let http = self.clone();
                let grpc = self;
                let http_addr = format!("0.0.0.0:{http_port}");
                let grpc_addr = format!("0.0.0.0:{grpc_port}");

                println!("Starting HTTP server on {http_addr}");
                println!("Starting gRPC server on {grpc_addr}");

                let http_task =
                    tokio::spawn(async move { http.serve_http(&http_addr).await });
                let grpc_task =
                    tokio::spawn(async move { grpc.serve_grpc(&grpc_addr).await });

                let (http_res, grpc_res) = tokio::join!(http_task, grpc_task);
                http_res.map_err(std::io::Error::other)??;
                grpc_res.map_err(std::io::Error::other)??;
                Ok(())
            }
            (Some(http_port), None) => {
                let addr = format!("0.0.0.0:{http_port}");
                println!("Starting HTTP server on {addr}");
                self.serve_http(&addr).await
            }
            (None, Some(grpc_port)) => {
                let addr = format!("0.0.0.0:{grpc_port}");
                println!("Starting gRPC server on {addr}");
                self.serve_grpc(&addr).await
            }
            (None, None) => unreachable!("validated above"),
        }
    }
}

fn parse_http_port(addr: &str) -> Option<u16> {
    if let Ok(socket) = addr.parse::<SocketAddr>() {
        return Some(socket.port());
    }
    addr.rsplit(':')
        .next()
        .and_then(|p| p.parse().ok())
}
