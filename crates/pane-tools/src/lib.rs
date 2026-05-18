pub mod file_ops;
pub mod search;
pub mod shell;

use serde::{Deserialize, Serialize};

/// A tool call from an agent that we need to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// The result of executing a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub id: String,
    pub output: String,
    pub is_error: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("execution failed: {0}")]
    ExecutionFailed(String),
}

pub async fn execute_tool(call: &ToolCall) -> ToolResult {
    let result = match call.name.as_str() {
        "read_file" => file_ops::read_file(&call.input).await,
        "write_file" => file_ops::write_file(&call.input).await,
        "list_directory" => file_ops::list_directory(&call.input).await,
        "search_files" => search::search_files(&call.input).await,
        "run_command" => shell::run_command(&call.input).await,
        other => Err(ToolError::UnknownTool(other.to_string())),
    };

    match result {
        Ok(output) => ToolResult {
            id: call.id.clone(),
            output,
            is_error: false,
        },
        Err(e) => ToolResult {
            id: call.id.clone(),
            output: e.to_string(),
            is_error: true,
        },
    }
}
