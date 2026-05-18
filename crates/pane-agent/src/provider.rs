use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio_stream::Stream;

use crate::session::AgentSession;
use pane_tools::ToolCall;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentEvent {
    /// Text content streaming from the model.
    Text(String),
    /// The model wants to call a tool.
    ToolUse(ToolCall),
    /// A tool result was produced.
    ToolResult(pane_tools::ToolResult),
    /// The turn is complete.
    Done,
    /// An error occurred.
    Error(String),
}

#[async_trait]
pub trait AgentProvider: Send + Sync {
    /// Send a user message and get a stream of events back.
    async fn send(
        &self,
        session: &mut AgentSession,
        message: &str,
    ) -> Result<Box<dyn Stream<Item = AgentEvent> + Send + Unpin>, AgentError>;
}

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("api error: {0}")]
    Api(String),
    #[error("provider not configured: {0}")]
    NotConfigured(String),
    #[error("session error: {0}")]
    Session(String),
}
