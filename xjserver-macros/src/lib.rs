//! Proc-macros for [`xjserver`](https://docs.rs/xjserver) route handlers.
//!
//! Re-exported from the `xjserver` crate — prefer `use xjserver::{xj_route, …}`.

mod args;
mod can_execute;
mod register;
mod route;

use proc_macro::TokenStream;

/// Rewrite an extractor-style gate into `async fn name<In>(ctx: &mut Context<In>) -> bool`.
///
/// Extractor failures return `false` (fail-closed).
///
/// ```ignore
/// #[xj_can_execute]
/// async fn is_authenticated(Session(s): Session) -> bool {
///     !s.is_guest()
/// }
/// ```
#[proc_macro_attribute]
pub fn xj_can_execute(attr: TokenStream, item: TokenStream) -> TokenStream {
    can_execute::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Generate a ZST + `impl XJRoute` for the `xj` bucket.
///
/// ```ignore
/// #[xj_route(name = "whoami", can_execute = is_authenticated)]
/// async fn whoami(Session(s): Session) -> Result<WhoAmIOut, XJError> { … }
/// // → discover (register=true default) or `.route(whoami)`
/// ```
///
/// Attrs: `name = "…"`, `can_execute = path`, `register = true|false` (default `true`).
#[proc_macro_attribute]
pub fn xj_route(attr: TokenStream, item: TokenStream) -> TokenStream {
    route::expand_route(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Like [`xj_route`], for the `login` bucket (`.login(…)` / discover).
#[proc_macro_attribute]
pub fn xj_login(attr: TokenStream, item: TokenStream) -> TokenStream {
    route::expand_named_bucket("login", attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Like [`xj_route`], for the `session` bucket (`.session(…)` / discover).
#[proc_macro_attribute]
pub fn xj_session(attr: TokenStream, item: TokenStream) -> TokenStream {
    route::expand_named_bucket("session", attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Like [`xj_route`], for the `logout` bucket (`.logout(…)` / discover).
#[proc_macro_attribute]
pub fn xj_logout(attr: TokenStream, item: TokenStream) -> TokenStream {
    route::expand_named_bucket("logout", attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Submit an existing `impl XJRoute` type into inventory for auto-discover.
///
/// Requires the `discover` feature on `xjserver`. Argument: `xj` | `login` |
/// `session` | `logout`.
///
/// ```ignore
/// #[xj_register(xj)]
/// struct ManualHello;
/// ```
#[proc_macro_attribute]
pub fn xj_register(attr: TokenStream, item: TokenStream) -> TokenStream {
    register::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
