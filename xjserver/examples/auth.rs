//! JWT login, authenticated routes, session, and logout.
//!
//! Run: `cargo run -p xjserver --example auth`
//!
//! ```bash
//! # Login (emits x-xj-token)
//! curl -si -X POST http://127.0.0.1:3002/login/password \
//!   -H 'content-type: application/json' \
//!   -d '{"user":"ada@gmail.com","password":"secret"}'
//!
//! # Authenticated whoami (paste token from login)
//! curl -s -X POST http://127.0.0.1:3002/xj/whoami \
//!   -H 'content-type: application/json' \
//!   -H 'x-xj-token: Bearer <token>' \
//!   -d '{}'
//!
//! # Guest whoami → 403
//! curl -s -X POST http://127.0.0.1:3002/xj/whoami \
//!   -H 'content-type: application/json' -d '{}'
//!
//! curl -s -X POST http://127.0.0.1:3002/session/me \
//!   -H 'content-type: application/json' \
//!   -H 'x-xj-token: Bearer <token>' -d '{}'
//!
//! curl -s -X POST http://127.0.0.1:3002/logout/bye \
//!   -H 'content-type: application/json' \
//!   -H 'x-xj-token: Bearer <token>' -d '{}'
//! ```

mod common;

#[path = "common/auth.rs"]
mod routes;

use routes::{LogoutBye, PasswordLogin, SessionMe, WhoAmI};
use common::{AppState, DEV_JWT_SECRET};
use xjserver::{XJConfig, XJServer};

const HTTP_ADDR: &str = "127.0.0.1:3002";

#[tokio::main]
async fn main() {
    let config = XJConfig {
        service_name: Some("auth".into()),
        jwt_secret: Some(DEV_JWT_SECRET.into()),
        jwt_expires_in: "10h".into(),
        ..XJConfig::default()
    };

    let server = XJServer::builder()
        .config(config)
        .login(PasswordLogin)
        .route(WhoAmI)
        .session(SessionMe)
        .logout(LogoutBye)
        .state(AppState::demo("auth"))
        .expect("register routes");

    println!("auth listening on http://{HTTP_ADDR}");
    server.serve_http(HTTP_ADDR).await.expect("serve");
}
