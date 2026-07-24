use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::context::ContextBase;
use crate::error::XJError;
use crate::manifest::{RouteClassInfo, route_class_info_from_type_name};
use crate::metadata::Metadata;
use crate::route::XJRoute;
use crate::validation::{json_schema_for, validate_json};

pub struct RouteOutcome {
    pub body: Value,
    pub metadata: Metadata,
}

/// XJ-RPC namespace / bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RouteBucket {
    Xj,
    Login,
    Session,
    Logout,
}

impl RouteBucket {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Xj => "xj",
            Self::Login => "login",
            Self::Session => "session",
            Self::Logout => "logout",
        }
    }

    /// gRPC service name (paridad Node: `Xj` | `Login` | `Session` | `Logout`).
    pub fn grpc_service_name(self) -> &'static str {
        match self {
            Self::Xj => "Xj",
            Self::Login => "Login",
            Self::Session => "Session",
            Self::Logout => "Logout",
        }
    }

    pub fn from_grpc_service_name(name: &str) -> Option<Self> {
        match name {
            "Xj" => Some(Self::Xj),
            "Login" => Some(Self::Login),
            "Session" => Some(Self::Session),
            "Logout" => Some(Self::Logout),
            _ => None,
        }
    }

    pub fn from_path_segment(segment: &str) -> Option<Self> {
        match segment {
            "xj" => Some(Self::Xj),
            "login" => Some(Self::Login),
            "session" => Some(Self::Session),
            "logout" => Some(Self::Logout),
            _ => None,
        }
    }
}

#[async_trait]
pub trait ErasedRoute: Send + Sync {
    fn name(&self) -> &'static str;

    fn route_type_name(&self) -> &'static str;

    fn route_class_info(&self) -> RouteClassInfo {
        route_class_info_from_type_name(self.route_type_name())
    }

    fn input_json_schema(&self) -> Option<Value>;

    fn output_json_schema(&self) -> Option<Value>;

    async fn dispatch(&self, body: &[u8], base: ContextBase) -> Result<RouteOutcome, XJError>;
}

struct TypedRoute<R> {
    inner: R,
}

/// Type-erase an [`XJRoute`] into an [`ErasedRoute`] (for inventory / `add_erased`).
pub fn erase<R>(route: R) -> Arc<dyn ErasedRoute>
where
    R: XJRoute + 'static,
    R::In: Send + Sync,
    R::Out: Send + Sync,
{
    Arc::new(TypedRoute { inner: route })
}

#[async_trait]
impl<R> ErasedRoute for TypedRoute<R>
where
    R: XJRoute,
    R::In: Send + Sync,
    R::Out: Send + Sync,
{
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn route_type_name(&self) -> &'static str {
        std::any::type_name::<R>()
    }

    fn input_json_schema(&self) -> Option<Value> {
        Some(json_schema_for::<R::In>())
    }

    fn output_json_schema(&self) -> Option<Value> {
        Some(json_schema_for::<R::Out>())
    }

    async fn dispatch(&self, body: &[u8], base: ContextBase) -> Result<RouteOutcome, XJError> {
        let route_name = self.inner.name();

        let raw: Value = if body.is_empty() {
            return Err(XJError::bad_request("Request body is required"));
        } else {
            serde_json::from_slice(body).map_err(|err| {
                XJError::bad_request(format!("Invalid JSON body: {err}"))
            })?
        };

        validate_json::<R::In>(&raw, route_name)?;

        let data: R::In = serde_json::from_value(raw).map_err(|err| {
            XJError::bad_request(format!("Invalid JSON body: {err}"))
        })?;

        let mut ctx = base.with_data(data);

        if !self.inner.can_execute(&mut ctx).await {
            return Err(XJError::forbidden("Forbidden"));
        }

        let out = self.inner.execute(&mut ctx).await?;
        let body = serde_json::to_value(&out)
            .map_err(|err| XJError::internal(format!("Failed to serialize response: {err}")))?;

        Ok(RouteOutcome {
            body,
            metadata: std::mem::take(ctx.metadata_mut()),
        })
    }
}

#[derive(Default)]
pub struct RouteRegistry {
    xj: HashMap<&'static str, Arc<dyn ErasedRoute>>,
    login: HashMap<&'static str, Arc<dyn ErasedRoute>>,
    session: HashMap<&'static str, Arc<dyn ErasedRoute>>,
    logout: HashMap<&'static str, Arc<dyn ErasedRoute>>,
}

impl RouteRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn contains_name(&self, name: &str) -> bool {
        self.xj.contains_key(name)
            || self.login.contains_key(name)
            || self.session.contains_key(name)
            || self.logout.contains_key(name)
    }

    fn bucket_mut(
        &mut self,
        bucket: RouteBucket,
    ) -> &mut HashMap<&'static str, Arc<dyn ErasedRoute>> {
        match bucket {
            RouteBucket::Xj => &mut self.xj,
            RouteBucket::Login => &mut self.login,
            RouteBucket::Session => &mut self.session,
            RouteBucket::Logout => &mut self.logout,
        }
    }

    fn bucket(&self, bucket: RouteBucket) -> &HashMap<&'static str, Arc<dyn ErasedRoute>> {
        match bucket {
            RouteBucket::Xj => &self.xj,
            RouteBucket::Login => &self.login,
            RouteBucket::Session => &self.session,
            RouteBucket::Logout => &self.logout,
        }
    }

    fn add_to_bucket<R>(&mut self, bucket: RouteBucket, route: R) -> Result<(), XJError>
    where
        R: XJRoute + 'static,
        R::In: Send + Sync,
        R::Out: Send + Sync,
    {
        self.add_erased(bucket, erase(route))
    }

    /// Insert an already type-erased route into `bucket`.
    ///
    /// Used by inventory discover and by callers that build `ErasedRoute`s themselves.
    pub fn add_erased(
        &mut self,
        bucket: RouteBucket,
        route: Arc<dyn ErasedRoute>,
    ) -> Result<(), XJError> {
        let name = route.name();
        if self.contains_name(name) {
            return Err(XJError::internal(format!(
                "Duplicate route registration: {name} (global uniqueness across buckets)"
            )));
        }

        self.bucket_mut(bucket).insert(name, route);
        Ok(())
    }

    pub fn add_xj<R>(&mut self, route: R) -> Result<(), XJError>
    where
        R: XJRoute + 'static,
        R::In: Send + Sync,
        R::Out: Send + Sync,
    {
        self.add_to_bucket(RouteBucket::Xj, route)
    }

    pub fn add_login<R>(&mut self, route: R) -> Result<(), XJError>
    where
        R: XJRoute + 'static,
        R::In: Send + Sync,
        R::Out: Send + Sync,
    {
        self.add_to_bucket(RouteBucket::Login, route)
    }

    pub fn add_session<R>(&mut self, route: R) -> Result<(), XJError>
    where
        R: XJRoute + 'static,
        R::In: Send + Sync,
        R::Out: Send + Sync,
    {
        self.add_to_bucket(RouteBucket::Session, route)
    }

    pub fn add_logout<R>(&mut self, route: R) -> Result<(), XJError>
    where
        R: XJRoute + 'static,
        R::In: Send + Sync,
        R::Out: Send + Sync,
    {
        self.add_to_bucket(RouteBucket::Logout, route)
    }

    pub fn get(&self, bucket: RouteBucket, name: &str) -> Option<Arc<dyn ErasedRoute>> {
        self.bucket(bucket).get(name).cloned()
    }

    pub fn routes_in_bucket(&self, bucket: RouteBucket) -> Vec<Arc<dyn ErasedRoute>> {
        let mut routes: Vec<Arc<dyn ErasedRoute>> =
            self.bucket(bucket).values().cloned().collect();
        routes.sort_by_key(|route| route.name());
        routes
    }

    pub fn get_xj(&self, name: &str) -> Option<Arc<dyn ErasedRoute>> {
        self.get(RouteBucket::Xj, name)
    }

    pub fn get_login(&self, name: &str) -> Option<Arc<dyn ErasedRoute>> {
        self.get(RouteBucket::Login, name)
    }

    pub fn get_session(&self, name: &str) -> Option<Arc<dyn ErasedRoute>> {
        self.get(RouteBucket::Session, name)
    }

    pub fn get_logout(&self, name: &str) -> Option<Arc<dyn ErasedRoute>> {
        self.get(RouteBucket::Logout, name)
    }
}
