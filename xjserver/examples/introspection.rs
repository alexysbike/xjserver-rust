//! Manifest, health, explorer docs, CORS, and rate limiting.
//!
//! Run: `cargo run -p xjserver --example introspection`
//!
//! ```bash
//! curl -s http://127.0.0.1:3003/health
//! curl -s http://127.0.0.1:3003/__xj/manifest | jq .
//! open http://127.0.0.1:3003/__xj/docs
//!
//! # Validation error (missing required field)
//! curl -s -X POST http://127.0.0.1:3003/login/password \
//!   -H 'content-type: application/json' -d '{"user":"ada@gmail.com"}'
//!
//! curl -si -X POST http://127.0.0.1:3003/login/password \
//!   -H 'content-type: application/json' \
//!   -d '{"user":"ada@gmail.com","password":"secret"}'
//! ```

mod common;

#[path = "common/auth.rs"]
mod routes;

use routes::{LogoutBye, PasswordLogin, SessionMe, WhoAmI};
use common::{AppState, DEV_JWT_SECRET};
use xjserver::{HttpCorsConfig, IntrospectionConfig, XJConfig, XJServer};

const HTTP_ADDR: &str = "127.0.0.1:3003";

#[tokio::main]
async fn main() {
    let config = XJConfig::default()
        .service_name("introspection")
        .jwt_secret(DEV_JWT_SECRET)
        .http_with(|h| h.port(3003).cors(HttpCorsConfig::Default))
        .introspection(IntrospectionConfig::enabled());

    let server = XJServer::builder()
        .config(config)
        .login(PasswordLogin)
        .route(WhoAmI)
        .session(SessionMe)
        .logout(LogoutBye)
        .state(AppState::demo("introspection"))
        .expect("register routes");

    println!("introspection listening on http://{HTTP_ADDR}");
    println!("  GET /health");
    println!("  GET /__xj/manifest");
    println!("  GET /__xj/docs");
    server.serve_http(HTTP_ADDR).await.expect("serve");
}
