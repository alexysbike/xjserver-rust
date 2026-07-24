//! Auth routes implemented with the [`XJRoute`] trait.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use xjserver::{Context, XJError, XJRoute};

use crate::common::{AppState, DEMO_EMAIL, DEMO_PASSWORD};

#[derive(Debug, Deserialize, JsonSchema)]
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
        let data = ctx.data();
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

#[derive(Debug, Deserialize, JsonSchema)]
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
        !ctx.session().is_guest()
    }

    async fn execute(&self, ctx: &mut Context<EmptyIn>) -> Result<WhoAmIOut, XJError> {
        let service = ctx
            .state::<AppState>()
            .map(|s| s.service.clone())
            .unwrap_or_else(|| "unknown".into());
        Ok(WhoAmIOut {
            name: ctx.session().name.clone(),
            id: ctx.session().id,
            guest: ctx.session().is_guest(),
            service,
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
        Ok(MeOut {
            name: ctx.session().name.clone(),
            id: ctx.session().id,
            claims: ctx.session().claims.clone(),
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
        Ok(ByeOut {
            ok: true,
            was_guest: ctx.session().is_guest(),
        })
    }
}
