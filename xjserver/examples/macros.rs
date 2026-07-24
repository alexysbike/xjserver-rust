//! Route proc-macros with explicit `.route` / `.login` registration.
//!
//! Run: `cargo run -p xjserver --example macros`
//!
//! ```bash
//! curl -si -X POST http://127.0.0.1:3006/login/password \
//!   -H 'content-type: application/json' \
//!   -d '{"user":"ada@gmail.com","password":"secret"}'
//!
//! # reuse x-xj-token response header as-is (default token_header)
//! curl -si -X POST http://127.0.0.1:3006/xj/whoami \
//!   -H "x-xj-token: Bearer $JWT" \
//!   -H 'content-type: application/json' \
//!   -d '{}'
//! ```

mod common;

#[path = "common/handlers.rs"]
mod handlers;

use handlers::{bye, me, password, whoami};
use common::{AppState, DEV_JWT_SECRET};
use xjserver::{HttpCorsConfig, IntrospectionConfig, XJServer};

const HTTP_PORT: u16 = 3006;

#[tokio::main]
async fn main() {
    // Skip discover so inventory submits from the same macros do not
    // collide with explicit registration.
    let server = XJServer::builder()
        .skip_discover()
        .service_name("macros")
        .jwt_secret(DEV_JWT_SECRET)
        .http_port(HTTP_PORT)
        .cors(HttpCorsConfig::Default)
        .introspection(IntrospectionConfig::enabled())
        .login(password)
        .route(whoami)
        .session(me)
        .logout(bye)
        .state(AppState::demo("macros"))
        .expect("register routes");

    println!("macros HTTP  http://127.0.0.1:{HTTP_PORT}");
    println!("  GET /health  GET /__xj/manifest  GET /__xj/docs");
    println!("  register handlers with .route / .login / .session / .logout");

    server.run().await.expect("run");
}
