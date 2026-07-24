//! Request extractors (`FromContext`) — ergonomics over [`Context`](crate::Context).
//!
//! # Owned vs borrowing
//!
//! Prefer **owned** extractors first (`Data`, `Config`, `Session`, `State`, `Extension`,
//! `Metadata`). They clone / `Arc`-clone and release the borrow on `ctx` immediately.
//!
//! **Borrowing** extractors (`MetadataMut`, [`Ctx`], `&mut Context<_>`) must come **last**:
//! they keep a mutable loan on `ctx`, so later `from_context` calls will not compile.
//!
//! # `can_execute` policy
//!
//! On extraction failure in a gate, return `false` (fail-closed). In `execute`, use `?`
//! so the rejection becomes [`XJError`].

use std::convert::Infallible;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::config::XJConfig;
use crate::context::Context;
use crate::error::XJError;
use crate::metadata::Metadata as Meta;
use crate::session::Session as Sess;

/// Empty JSON object body (`{}`).
///
/// Default [`crate::XJRoute::In`] when a `#[xj_route]` / `#[xj_login]` / … handler
/// has no [`Data<T>`] extractor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Empty {}

/// Extract a value from a mutable [`Context`].
///
/// Generic over route input `In` so gates/extractors that ignore the body work for any route.
pub trait FromContext<'a, In>: Sized {
    type Rejection: Into<XJError>;

    fn from_context(ctx: &'a mut Context<In>) -> Result<Self, Self::Rejection>;
}

/// Failure extracting a required piece of context (missing state / extension).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractionError {
    MissingState { type_name: &'static str },
    MissingExtension { type_name: &'static str },
}

impl ExtractionError {
    pub fn missing_state<T: 'static>() -> Self {
        Self::MissingState {
            type_name: std::any::type_name::<T>(),
        }
    }

    pub fn missing_extension<T: 'static>() -> Self {
        Self::MissingExtension {
            type_name: std::any::type_name::<T>(),
        }
    }
}

impl From<ExtractionError> for XJError {
    fn from(err: ExtractionError) -> Self {
        match err {
            ExtractionError::MissingState { type_name } => {
                XJError::internal(format!("missing app state `{type_name}`"))
            }
            ExtractionError::MissingExtension { type_name } => {
                XJError::internal(format!("missing extension `{type_name}`"))
            }
        }
    }
}

impl From<Infallible> for XJError {
    fn from(err: Infallible) -> Self {
        match err {}
    }
}

// ----- Data -----

/// Cloned route input (`T` must be the route's `In`).
#[derive(Debug, Clone)]
pub struct Data<T>(pub T);

impl<T> Deref for Data<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Data<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'a, In> FromContext<'a, In> for Data<In>
where
    In: Clone,
{
    type Rejection = Infallible;

    fn from_context(ctx: &'a mut Context<In>) -> Result<Self, Self::Rejection> {
        Ok(Data(ctx.data().clone()))
    }
}

// ----- Config -----

/// Shared server config (`Arc` clone).
#[derive(Clone)]
pub struct Config(pub Arc<XJConfig>);

impl Deref for Config {
    type Target = XJConfig;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a, In> FromContext<'a, In> for Config {
    type Rejection = Infallible;

    fn from_context(ctx: &'a mut Context<In>) -> Result<Self, Self::Rejection> {
        Ok(Config(ctx.config_arc()))
    }
}

// ----- Session -----

/// Cloned request [`crate::Session`].
#[derive(Debug, Clone)]
pub struct Session(pub Sess);

impl Deref for Session {
    type Target = Sess;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Session {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'a, In> FromContext<'a, In> for Session {
    type Rejection = Infallible;

    fn from_context(ctx: &'a mut Context<In>) -> Result<Self, Self::Rejection> {
        Ok(Session(ctx.session().clone()))
    }
}

// ----- State -----

/// Cloned typed app state.
#[derive(Debug, Clone)]
pub struct State<S>(pub S);

impl<S> Deref for State<S> {
    type Target = S;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> DerefMut for State<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'a, In, S> FromContext<'a, In> for State<S>
where
    S: Clone + Send + Sync + 'static,
{
    type Rejection = ExtractionError;

    fn from_context(ctx: &'a mut Context<In>) -> Result<Self, Self::Rejection> {
        ctx.state::<S>()
            .cloned()
            .map(State)
            .ok_or_else(ExtractionError::missing_state::<S>)
    }
}

// ----- Extension -----

/// Cloned value from [`Context`] extensions (`http::Extensions`).
#[derive(Debug, Clone)]
pub struct Extension<T>(pub T);

impl<T> Deref for Extension<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Extension<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'a, In, T> FromContext<'a, In> for Extension<T>
where
    T: Clone + Send + Sync + 'static,
{
    type Rejection = ExtractionError;

    fn from_context(ctx: &'a mut Context<In>) -> Result<Self, Self::Rejection> {
        ctx.get::<T>()
            .cloned()
            .map(Extension)
            .ok_or_else(ExtractionError::missing_extension::<T>)
    }
}

// ----- Metadata (owned) -----

/// Cloned request/response metadata bag (read snapshot).
#[derive(Debug, Clone)]
pub struct Metadata(pub Meta);

impl Deref for Metadata {
    type Target = Meta;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Metadata {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'a, In> FromContext<'a, In> for Metadata {
    type Rejection = Infallible;

    fn from_context(ctx: &'a mut Context<In>) -> Result<Self, Self::Rejection> {
        Ok(Metadata(ctx.metadata().clone()))
    }
}

// ----- MetadataMut (borrow) -----

/// Mutable borrow of context metadata. Use **last** among extractors.
#[derive(Debug)]
pub struct MetadataMut<'a>(pub &'a mut Meta);

impl Deref for MetadataMut<'_> {
    type Target = Meta;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl DerefMut for MetadataMut<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

impl<'a, In> FromContext<'a, In> for MetadataMut<'a> {
    type Rejection = Infallible;

    fn from_context(ctx: &'a mut Context<In>) -> Result<Self, Self::Rejection> {
        Ok(MetadataMut(ctx.metadata_mut()))
    }
}

// ----- Ctx / &mut Context (borrow) -----

/// Passthrough mutable borrow of the full context. Use **last** among extractors.
pub struct Ctx<'a, In>(pub &'a mut Context<In>);

impl<In> Deref for Ctx<'_, In> {
    type Target = Context<In>;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<In> DerefMut for Ctx<'_, In> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

impl<'a, In> FromContext<'a, In> for Ctx<'a, In> {
    type Rejection = Infallible;

    fn from_context(ctx: &'a mut Context<In>) -> Result<Self, Self::Rejection> {
        Ok(Ctx(ctx))
    }
}

impl<'a, In> FromContext<'a, In> for &'a mut Context<In> {
    type Rejection = Infallible;

    fn from_context(ctx: &'a mut Context<In>) -> Result<Self, Self::Rejection> {
        Ok(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session as Sess;
    use http::Extensions;
    use std::sync::Arc;

    #[derive(Clone, Debug, PartialEq)]
    struct AppState {
        name: String,
    }

    #[derive(Clone, Debug, PartialEq)]
    struct TraceId(String);

    fn make_ctx(data: i32) -> Context<i32> {
        Context::new(
            data,
            Sess::guest(),
            Meta::default(),
            Arc::new(AppState {
                name: "test".into(),
            }),
            Arc::new(XJConfig::default()),
            Extensions::new(),
        )
    }

    #[test]
    fn data_config_session_state_extract() {
        let mut ctx = make_ctx(7);

        let Data(n) = Data::<i32>::from_context(&mut ctx).unwrap();
        assert_eq!(n, 7);

        let Config(cfg) = Config::from_context(&mut ctx).unwrap();
        assert!(cfg.service_name.is_none());

        let Session(s) = Session::from_context(&mut ctx).unwrap();
        assert!(s.is_guest());

        let State(app) = State::<AppState>::from_context(&mut ctx).unwrap();
        assert_eq!(app.name, "test");
    }

    #[test]
    fn state_missing_rejects() {
        let mut ctx = Context::new(
            (),
            Sess::guest(),
            Meta::default(),
            Arc::new(()),
            Arc::new(XJConfig::default()),
            Extensions::new(),
        );

        let err = State::<AppState>::from_context(&mut ctx).unwrap_err();
        assert!(matches!(err, ExtractionError::MissingState { .. }));
        let xj: XJError = err.into();
        assert_eq!(xj.status_code(), 500);
    }

    #[test]
    fn extension_roundtrip() {
        let mut ctx = make_ctx(1);
        ctx.insert(TraceId("abc".into()));

        let Extension(id) = Extension::<TraceId>::from_context(&mut ctx).unwrap();
        assert_eq!(id.0, "abc");
    }

    #[test]
    fn extension_missing_rejects() {
        let mut ctx = make_ctx(1);
        let err = Extension::<TraceId>::from_context(&mut ctx).unwrap_err();
        assert!(matches!(err, ExtractionError::MissingExtension { .. }));
    }

    #[test]
    fn metadata_owned_then_mut() {
        let mut ctx = make_ctx(1);
        ctx.metadata_mut()
            .set_outgoing("x-trace", "1");

        let Metadata(snap) = Metadata::from_context(&mut ctx).unwrap();
        assert!(snap.has_outgoing());

        {
            let MetadataMut(meta) = MetadataMut::from_context(&mut ctx).unwrap();
            meta.set_outgoing("x-trace", "2");
        }
        assert_eq!(
            ctx.metadata().outgoing().get("x-trace").map(String::as_str),
            Some("2")
        );
    }

    #[test]
    fn ctx_passthrough() {
        let mut ctx = make_ctx(9);
        let Ctx(inner) = Ctx::from_context(&mut ctx).unwrap();
        assert_eq!(*inner.data(), 9);
    }

    #[test]
    fn can_execute_fail_closed_pattern() {
        fn gate<In>(ctx: &mut Context<In>) -> bool {
            let State(app) = match State::<AppState>::from_context(ctx) {
                Ok(v) => v,
                Err(_) => return false,
            };
            app.name == "test"
        }

        let mut ok_ctx = make_ctx(0);
        assert!(gate(&mut ok_ctx));

        let mut bad_ctx = Context::new(
            0,
            Sess::guest(),
            Meta::default(),
            Arc::new(()),
            Arc::new(XJConfig::default()),
            Extensions::new(),
        );
        assert!(!gate(&mut bad_ctx));
    }
}
