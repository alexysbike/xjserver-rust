//! XJServer — RPC framework (work in progress).

// Allows proc-macro expansions (`::xjserver::…`) to resolve inside this crate.
extern crate self as xjserver;

mod config;
mod context;
#[cfg(feature = "discover")]
pub mod discover;
mod error;
pub mod extract;
mod grpc;
mod http;
mod login_token_middleware;
mod manifest;
mod metadata;
mod registry;
mod route;
mod server;
mod session;
mod session_middleware;
mod validation;

#[cfg(test)]
mod lib_tests;

pub use config::{
    CustomCorsConfig, ExplorerIntrospectionConfig, GrpcConfig, HealthCheck, HealthCheckResult,
    HealthIntrospectionConfig, HttpConfig, HttpCorsConfig, IntrospectionConfig,
    ManifestClientConfig, ManifestIntrospectionConfig, XJConfig,
};
pub use context::{Context, ContextBase};
pub use error::{RouteValidationIssue, XJError};
pub use extract::{Ctx, Data, Empty, Extension, ExtractionError, FromContext, MetadataMut};
pub use grpc::{
    DEFAULT_PROTO_PACKAGE, GenerateProtoOptions, GeneratedProto, RouteProtoShape,
    generate_proto_from_registry, prepare_grpc, write_generated_proto,
};
pub use login_token_middleware::{JwtLoginTokenMiddleware, LoginTokenMiddleware};
pub use manifest::{XJManifest, XJ_MANIFEST_VERSION, build_manifest};
pub use metadata::Metadata;
pub use registry::{erase, ErasedRoute, RouteBucket, RouteOutcome, RouteRegistry};
pub use route::XJRoute;
pub use server::{XJServer, XJServerBuilder};
pub use session::Session;
pub use session_middleware::{JwtSessionMiddleware, SessionMiddleware};

#[cfg(feature = "discover")]
pub use discover::RouteRegistration;

/// Proc-macros for function-style routes.
pub use xjserver_macros::{
    xj_can_execute, xj_login, xj_logout, xj_register, xj_route, xj_session,
};

/// Re-export used by `xjserver-macros` expansions (`#[xjserver::__async_trait]`).
#[doc(hidden)]
pub use async_trait::async_trait as __async_trait;

/// Re-export used by `xjserver-macros` inventory submits (feature `discover`).
#[cfg(feature = "discover")]
#[doc(hidden)]
pub use inventory as __inventory;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
