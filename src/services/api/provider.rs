use async_trait::async_trait;
use futures::stream::BoxStream;

use super::types::*;

/// Stream of events from LLM provider
pub type MessageStream = BoxStream<'static, crate::types::Result<StreamEvent>>;

/// Core LLM provider trait — all providers implement this
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Create a streaming message
    async fn create_message(
        &self,
        request: CreateMessageRequest,
    ) -> crate::types::Result<MessageStream>;

    /// Get model information
    fn model_info(&self) -> &ModelInfo;

    /// Get provider capabilities
    fn capabilities(&self) -> &Capabilities;
}
