//! Basic HTTP routes with the [`XJRoute`] trait.
//!
//! Run: `cargo run -p xjserver --example hello`
//!
//! ```bash
//! curl -s -X POST http://127.0.0.1:3001/xj/hello -H 'content-type: application/json' -d '{"name":"Ada"}'
//! curl -s -X POST http://127.0.0.1:3001/xj/admin_echo -H 'content-type: application/json' -d '{"message":"nope","admin":false}'
//! curl -s -X POST http://127.0.0.1:3001/xj/admin_echo -H 'content-type: application/json' -d '{"message":"secret","admin":true}'
//! ```

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use xjserver::{Context, Session, XJConfig, XJError, XJRoute, XJServer};

const HTTP_ADDR: &str = "127.0.0.1:3001";

#[derive(Clone)]
struct AppState {
    greeting_prefix: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct HelloIn {
    name: String,
}

#[derive(Debug, Serialize, JsonSchema)]
struct HelloOut {
    message: String,
    session_id: i64,
}

struct HelloRoute;

#[async_trait]
impl XJRoute for HelloRoute {
    type In = HelloIn;
    type Out = HelloOut;

    fn name(&self) -> &'static str {
        "hello"
    }

    async fn execute(&self, ctx: &mut Context<HelloIn>) -> Result<HelloOut, XJError> {
        let prefix = ctx
            .state::<AppState>()
            .map(|s| s.greeting_prefix.clone())
            .unwrap_or_else(|| "Hello".to_string());
        let name = ctx.data().name.clone();
        ctx.metadata_mut().set_outgoing("x-xj-example", "hello");
        Ok(HelloOut {
            message: format!("{prefix}, {name}!"),
            session_id: ctx.session().id,
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct AdminIn {
    message: String,
    admin: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
struct AdminOut {
    echo: String,
}

struct AdminEchoRoute;

#[async_trait]
impl XJRoute for AdminEchoRoute {
    type In = AdminIn;
    type Out = AdminOut;

    fn name(&self) -> &'static str {
        "admin_echo"
    }

    async fn can_execute(&self, ctx: &mut Context<AdminIn>) -> bool {
        ctx.data().admin
    }

    async fn execute(&self, ctx: &mut Context<AdminIn>) -> Result<AdminOut, XJError> {
        let _session: &Session = ctx.session();
        Ok(AdminOut {
            echo: ctx.data().message.clone(),
        })
    }
}

#[tokio::main]
async fn main() {
    let config = XJConfig {
        service_name: Some("hello".into()),
        ..XJConfig::default()
    };

    let server = XJServer::builder()
        .config(config)
        .route(HelloRoute)
        .route(AdminEchoRoute)
        .state(AppState {
            greeting_prefix: "Hello".into(),
        })
        .expect("register routes");

    println!("hello listening on http://{HTTP_ADDR}");
    server.serve_http(HTTP_ADDR).await.expect("serve");
}
