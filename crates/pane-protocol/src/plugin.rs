use serde::{Deserialize, Serialize};

/// Configuration for a single plugin.
#[derive(Clone, Debug)]
pub struct PluginConfig {
    pub command: String,
    pub events: Vec<String>,
    pub refresh_interval_secs: u64,
}

/// A segment of text produced by a plugin for the status bar.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginSegment {
    pub text: String,
    #[serde(default = "default_style")]
    pub style: String,
}

fn default_style() -> String {
    "dim".to_string()
}
