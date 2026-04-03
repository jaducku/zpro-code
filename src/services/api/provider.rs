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

/// No-op provider for tests. Returns an empty stream immediately.
#[cfg(test)]
pub struct NoopProvider;

#[cfg(test)]
#[async_trait]
impl LlmProvider for NoopProvider {
    async fn create_message(
        &self,
        _request: CreateMessageRequest,
    ) -> crate::types::Result<MessageStream> {
        Ok(Box::pin(futures::stream::empty()))
    }
    fn model_info(&self) -> &ModelInfo {
        Box::leak(Box::new(ModelInfo {
            id: "noop".into(),
            name: "noop".into(),
            context_window: 128_000,
            max_output_tokens: 4096,
            capabilities: Capabilities::default(),
        }))
    }
    fn capabilities(&self) -> &Capabilities {
        Box::leak(Box::new(Capabilities::default()))
    }
}
