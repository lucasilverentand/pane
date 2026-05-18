use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: String,
    pub name: String,
    pub project_root: PathBuf,
    pub provider: String,
    pub model: String,
    pub messages: Vec<Message>,
    pub status: AgentStatus,
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    Idle,
    Running,
    WaitingForTool,
    Error,
}

impl AgentSession {
    pub fn new(
        name: impl Into<String>,
        project_root: PathBuf,
        provider: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid_v4(),
            name: name.into(),
            project_root,
            provider: provider.into(),
            model: model.into(),
            messages: Vec::new(),
            status: AgentStatus::Idle,
            system_prompt: None,
        }
    }

    pub fn push_message(&mut self, role: Role, content: impl Into<String>) {
        self.messages.push(Message {
            role,
            content: content.into(),
        });
    }
}

fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    // Simple unique-enough ID without pulling in the uuid crate
    format!("agent-{ts:x}")
}
