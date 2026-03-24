use serde::{Deserialize, Serialize};

pub type WindowId = uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabKind {
    Shell,
    Agent,
    Nvim,
    DevServer,
}

impl TabKind {
    pub fn label(&self) -> &str {
        match self {
            TabKind::Shell => "shell",
            TabKind::Agent => "claude",
            TabKind::Nvim => "nvim",
            TabKind::DevServer => "server",
        }
    }
}
