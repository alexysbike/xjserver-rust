use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::context::Context;
use crate::error::XJError;

#[async_trait]
pub trait XJRoute: Send + Sync {
    type In: DeserializeOwned + JsonSchema + Send + 'static;
    type Out: Serialize + JsonSchema + Send + 'static;

    fn name(&self) -> &'static str;

    /// Default: allow always.
    ///
    /// Takes `&mut` so [`crate::extract::FromContext`] works the same as in [`Self::execute`]
    /// (owned extractors only clone; gates should not mutate). On extractor failure, return
    /// `false` (fail-closed).
    async fn can_execute(&self, _ctx: &mut Context<Self::In>) -> bool {
        true
    }

    async fn execute(&self, ctx: &mut Context<Self::In>) -> Result<Self::Out, XJError>;
}
