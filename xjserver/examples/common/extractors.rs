//! Auth routes using manual [`FromContext`] extractors.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use xjserver::extract::{Config, Data, FromContext, MetadataMut, Session, State};
use xjserver::{Context, XJError, XJRoute};

use crate::common::{AppState, DEMO_EMAIL, DEMO_PASSWORD};

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LoginIn {
    #[schemars(email)]
    pub user: String,
    pub password: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct LoginOut {
    pub name: String,
    pub id: i64,
    pub role: String,
}

pub struct PasswordLogin;

#[async_trait]
impl XJRoute for PasswordLogin {
    type In = LoginIn;
    type Out = LoginOut;

    fn name(&self) -> &'static str {
        "password"
    }

    async fn execute(&self, ctx: &mut Context<LoginIn>) -> Result<LoginOut, XJError> {
        let Data(data) = Data::<LoginIn>::from_context(ctx)?;
        let Config(cfg) = Config::from_context(ctx)?;
        let _ = cfg.service_name.as_deref();

        if data.user != DEMO_EMAIL || data.password != DEMO_PASSWORD {
            return Err(XJError::forbidden("Invalid credentials"));
        }
        Ok(LoginOut {
            name: "ada".into(),
            id: 42,
            role: "admin".into(),
        })
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct EmptyIn {}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WhoAmIOut {
    pub name: String,
    pub id: i64,
    pub guest: bool,
    pub service: String,
}

pub struct WhoAmI;

#[async_trait]
impl XJRoute for WhoAmI {
    type In = EmptyIn;
    type Out = WhoAmIOut;

    fn name(&self) -> &'static str {
        "whoami"
    }

    async fn can_execute(&self, ctx: &mut Context<EmptyIn>) -> bool {
        let Session(s) = match Session::from_context(ctx) {
            Ok(v) => v,
            Err(_) => return false,
        };
        !s.is_guest()
    }

    async fn execute(&self, ctx: &mut Context<EmptyIn>) -> Result<WhoAmIOut, XJError> {
        let Session(s) = Session::from_context(ctx)?;
        let State(app) = State::<AppState>::from_context(ctx)?;
        let MetadataMut(meta) = MetadataMut::from_context(ctx)?;
        meta.set_outgoing("x-xj-example", "extractors");

        Ok(WhoAmIOut {
            name: s.name.clone(),
            id: s.id,
            guest: s.is_guest(),
            service: app.service,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MeOut {
    pub name: String,
    pub id: i64,
    pub claims: serde_json::Value,
}

pub struct SessionMe;

#[async_trait]
impl XJRoute for SessionMe {
    type In = EmptyIn;
    type Out = MeOut;

    fn name(&self) -> &'static str {
        "me"
    }

    async fn execute(&self, ctx: &mut Context<EmptyIn>) -> Result<MeOut, XJError> {
        let Session(s) = Session::from_context(ctx)?;
        Ok(MeOut {
            name: s.name,
            id: s.id,
            claims: s.claims,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ByeOut {
    pub ok: bool,
    pub was_guest: bool,
}

pub struct LogoutBye;

#[async_trait]
impl XJRoute for LogoutBye {
    type In = EmptyIn;
    type Out = ByeOut;

    fn name(&self) -> &'static str {
        "bye"
    }

    async fn execute(&self, ctx: &mut Context<EmptyIn>) -> Result<ByeOut, XJError> {
        let Session(s) = Session::from_context(ctx)?;
        Ok(ByeOut {
            ok: true,
            was_guest: s.is_guest(),
        })
    }
}
