//! Auto-registration of routes via [`inventory`] (`discover` feature).
//!
//! The `#[xj_route]` / `#[xj_login]` / … macros (with `register = true`, default) and
//! `#[xj_register(…)]` emit `inventory::submit!` of [`RouteRegistration`].
//! [`crate::XJServer::builder`] consumes them when calling [`.state`](crate::XJServerBuilder::state).
//!
//! # Do not mix `.route(x)` with `register = true`
//!
//! If the same route both submits **and** is registered with `.route(x)` / `.login(x)` / …,
//! `.state(...)` fails with a duplicate name. Per-route opt-out: `register = false`.
//!
//! # Linker (routes in another crate)
//!
//! `inventory` only sees submits **linked** into the final binary. If routes live
//! in a `lib` crate, the binary that runs discover must reference that crate
//! (e.g. `use my_routes as _;`) so the linker does not strip the submits.

use std::sync::Arc;

use crate::registry::{ErasedRoute, RouteBucket};

/// Inventory entry: bucket + factory that produces the type-erased route.
///
/// Emitted by macros when the `discover` feature is active and `register = true`.
pub struct RouteRegistration {
    pub bucket: RouteBucket,
    pub factory: fn() -> Arc<dyn ErasedRoute>,
}

inventory::collect!(RouteRegistration);
