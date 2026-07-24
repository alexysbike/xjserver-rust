use std::any::Any;
use std::sync::Arc;

use http::Extensions;

use crate::config::XJConfig;
use crate::metadata::Metadata;
use crate::session::Session;

/// Per-request bag: typed route input plus session/metadata/config/state/extensions.
pub struct Context<In> {
    data: In,
    session: Session,
    metadata: Metadata,
    /// App state (type-erased). Use [`Self::state`].
    state: Arc<dyn Any + Send + Sync>,
    config: Arc<XJConfig>,
    extensions: Extensions,
}

impl<In> Context<In> {
    pub fn new(
        data: In,
        session: Session,
        metadata: Metadata,
        state: Arc<dyn Any + Send + Sync>,
        config: Arc<XJConfig>,
        extensions: Extensions,
    ) -> Self {
        Self {
            data,
            session,
            metadata,
            state,
            config,
            extensions,
        }
    }

    pub fn data(&self) -> &In {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut In {
        &mut self.data
    }

    pub fn into_data(self) -> In {
        self.data
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    pub fn metadata_mut(&mut self) -> &mut Metadata {
        &mut self.metadata
    }

    pub fn config(&self) -> &XJConfig {
        &self.config
    }

    pub fn config_arc(&self) -> Arc<XJConfig> {
        Arc::clone(&self.config)
    }

    pub fn state<S: Send + Sync + 'static>(&self) -> Option<&S> {
        self.state.downcast_ref::<S>()
    }

    pub fn extensions(&self) -> &Extensions {
        &self.extensions
    }

    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.extensions
    }

    pub fn insert<T: Clone + Send + Sync + 'static>(&mut self, value: T) {
        self.extensions.insert(value);
    }

    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.extensions.get::<T>()
    }
}

/// Shared pieces used to build a typed [`Context`] after deserializing `In`.
pub struct ContextBase {
    pub session: Session,
    pub metadata: Metadata,
    pub state: Arc<dyn Any + Send + Sync>,
    pub config: Arc<XJConfig>,
    pub extensions: Extensions,
}

impl ContextBase {
    pub fn with_data<In>(self, data: In) -> Context<In> {
        Context::new(
            data,
            self.session,
            self.metadata,
            self.state,
            self.config,
            self.extensions,
        )
    }
}
