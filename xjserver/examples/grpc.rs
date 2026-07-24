//! HTTP + gRPC dual transport with runtime proto generation.
//!
//! Run: `cargo run -p xjserver --example grpc`
//!
//! ```bash
//! curl -si -X POST http://127.0.0.1:3004/login/password \
//!   -H 'content-type: application/json' \
//!   -d '{"user":"ada@gmail.com","password":"secret"}'
//!
//! # gRPC (requires grpcurl + the generated proto)
//! grpcurl -plaintext -proto target/xjserver-grpc.proto \
//!   -d '{"user":"ada@gmail.com","password":"secret"}' \
//!   127.0.0.1:50051 xjserver.v1.Login/password
//!
//! grpcurl -plaintext -proto target/xjserver-grpc.proto \
//!   -H 'x-xj-token: Bearer <token>' \
//!   -d '{}' \
//!   127.0.0.1:50051 xjserver.v1.Xj/whoami
//! ```

mod common;

#[path = "common/auth.rs"]
mod routes;

use routes::{LogoutBye, PasswordLogin, SessionMe, WhoAmI};
use common::{AppState, DEV_JWT_SECRET};
use xjserver::{HttpCorsConfig, IntrospectionConfig, XJServer};

const HTTP_PORT: u16 = 3004;
const GRPC_PORT: u16 = 50051;
const PROTO_PATH: &str = "target/xjserver-grpc.proto";

#[tokio::main]
async fn main() {
    let server = XJServer::builder()
        .service_name("grpc")
        .jwt_secret(DEV_JWT_SECRET)
        .http_port(HTTP_PORT)
        .cors(HttpCorsConfig::Default)
        .introspection(IntrospectionConfig::enabled())
        .grpc(GRPC_PORT, PROTO_PATH)
        .login(PasswordLogin)
        .route(WhoAmI)
        .session(SessionMe)
        .logout(LogoutBye)
        .state(AppState::demo("grpc"))
        .expect("register routes");

    println!("grpc HTTP  http://127.0.0.1:{HTTP_PORT}");
    println!("grpc gRPC  127.0.0.1:{GRPC_PORT}");
    println!("  proto → {PROTO_PATH}");
    println!("  GET /health  GET /__xj/manifest  GET /__xj/docs");

    server.run().await.expect("run");
}
