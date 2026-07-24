//! Auto-registration via `inventory` and the `discover` feature (default).
//!
//! Run: `cargo run -p xjserver --example discover`
//!
//! Macros with `register = true` (default) call `submit!`; `XJServer::builder()`
//! loads them in `.state(...)` — no explicit `.route` / `.login`.
//!
//! ```bash
//! curl -si -X POST http://127.0.0.1:3007/login/password \
//!   -H 'content-type: application/json' \
//!   -d '{"user":"ada@gmail.com","password":"secret"}'
//!
//! # reuse x-xj-token response header as-is (default token_header)
//! curl -si -X POST http://127.0.0.1:3007/xj/whoami \
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

const HTTP_PORT: u16 = 3007;

#[tokio::main]
async fn main() {
    let server = XJServer::builder()
        .service_name("discover")
        .jwt_secret(DEV_JWT_SECRET)
        .http_port(HTTP_PORT)
        .cors(HttpCorsConfig::Default)
        .introspection(IntrospectionConfig::enabled())
        .state(AppState::demo("discover"))
        .expect("discover + state");

    // Keep handler symbols linked for inventory discover.
    let _ = (password, whoami, me, bye);

    println!("discover HTTP  http://127.0.0.1:{HTTP_PORT}");
    println!("  GET /health  GET /__xj/manifest  GET /__xj/docs");
    println!("  inventory discover — no .route / .login calls");

    server.run().await.expect("run");
}
