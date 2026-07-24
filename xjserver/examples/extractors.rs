//! Manual [`FromContext`] extractors without proc-macros.
//!
//! Run: `cargo run -p xjserver --example extractors`
//!
//! ```bash
//! curl -si -X POST http://127.0.0.1:3005/login/password \
//!   -H 'content-type: application/json' \
//!   -d '{"user":"ada@gmail.com","password":"secret"}'
//!
//! TOKEN=… # from x-xj-token response header
//! curl -si -X POST http://127.0.0.1:3005/xj/whoami \
//!   -H "authorization: Bearer $TOKEN" \
//!   -H 'content-type: application/json' \
//!   -d '{}'
//! ```

mod common;

#[path = "common/extractors.rs"]
mod routes;

use routes::{LogoutBye, PasswordLogin, SessionMe, WhoAmI};
use common::{AppState, DEV_JWT_SECRET};
use xjserver::{HttpCorsConfig, IntrospectionConfig, XJServer};

const HTTP_PORT: u16 = 3005;

#[tokio::main]
async fn main() {
    let server = XJServer::builder()
        .service_name("extractors")
        .jwt_secret(DEV_JWT_SECRET)
        .http_port(HTTP_PORT)
        .cors(HttpCorsConfig::Default)
        .introspection(IntrospectionConfig::enabled())
        .login(PasswordLogin)
        .route(WhoAmI)
        .session(SessionMe)
        .logout(LogoutBye)
        .state(AppState::demo("extractors"))
        .expect("register routes");

    println!("extractors HTTP  http://127.0.0.1:{HTTP_PORT}");
    println!("  GET /health  GET /__xj/manifest  GET /__xj/docs");
    println!("  extractors used manually in execute / can_execute");

    server.run().await.expect("run");
}
