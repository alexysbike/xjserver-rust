//! Auth handlers implemented with route proc-macros.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use xjserver::extract::{Config, Data, MetadataMut, Session, State};
use xjserver::{XJError, xj_can_execute, xj_login, xj_logout, xj_route, xj_session};

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

#[xj_can_execute]
pub async fn is_authenticated(Session(s): Session) -> bool {
    !s.is_guest()
}

#[xj_login(name = "password")]
pub async fn password(
    Data(data): Data<LoginIn>,
    Config(cfg): Config,
) -> Result<LoginOut, XJError> {
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

#[derive(Debug, Serialize, JsonSchema)]
pub struct WhoAmIOut {
    pub name: String,
    pub id: i64,
    pub guest: bool,
    pub service: String,
}

#[xj_route(name = "whoami", can_execute = is_authenticated)]
pub async fn whoami(
    Session(s): Session,
    State(app): State<AppState>,
    MetadataMut(meta): MetadataMut<'_>,
) -> Result<WhoAmIOut, XJError> {
    meta.set_outgoing("x-xj-example", "handlers");

    Ok(WhoAmIOut {
        name: s.name.clone(),
        id: s.id,
        guest: s.is_guest(),
        service: app.service,
    })
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MeOut {
    pub name: String,
    pub id: i64,
    pub claims: serde_json::Value,
}

#[xj_session(name = "me")]
pub async fn me(Session(s): Session) -> Result<MeOut, XJError> {
    Ok(MeOut {
        name: s.name,
        id: s.id,
        claims: s.claims,
    })
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ByeOut {
    pub ok: bool,
    pub was_guest: bool,
}

#[xj_logout(name = "bye")]
pub async fn bye(Session(s): Session) -> Result<ByeOut, XJError> {
    Ok(ByeOut {
        ok: true,
        was_guest: s.is_guest(),
    })
}
