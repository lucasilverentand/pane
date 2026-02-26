//! Plugin system: spawn child processes that communicate via JSON stdin/stdout.

use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

// Re-export shared types from pane-protocol
pub use pane_protocol::plugin::{PluginConfig, PluginSegment};

/// JSON output from a plugin.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct PluginOutput {
    #[serde(default)]
    segments: Vec<PluginSegment>,
    #[serde(default)]
    commands: Vec<String>,
}

/// JSON input sent to a plugin.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct PluginInput {
    event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_stats: Option<PluginStats>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PluginStats {
    cpu_percent: f32,
    memory_percent: f32,
    load_avg_1: f64,
}

/// Messages from plugin manager to the main event loop.
pub enum PluginEvent {
    SegmentsUpdated {
        plugin_idx: usize,
        segments: Vec<PluginSegment>,
    },
    Commands {
        commands: Vec<String>,
    },
}

/// Manages all plugins for a session.
pub struct PluginManager {
    configs: Vec<PluginConfig>,
    children: Vec<Option<PluginChild>>,
    segments: Vec<Vec<PluginSegment>>,
    event_tx: mpsc::UnboundedSender<PluginEvent>,
}

struct PluginChild {
    child: Child,
    stdin: tokio::process::ChildStdin,
}

impl PluginManager {
    pub fn new(configs: Vec<PluginConfig>) -> (Self, mpsc::UnboundedReceiver<PluginEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let segments = vec![Vec::new(); configs.len()];
        let children: Vec<Option<PluginChild>> = (0..configs.len()).map(|_| None).collect();
        (
            Self {
                configs,
                children,
                segments,
                event_tx,
            },
            event_rx,
        )
    }

    pub fn start_all(&mut self) {
        for i in 0..self.configs.len() {
            self.start_plugin(i);
        }
    }

    fn start_plugin(&mut self, idx: usize) {
        let config = &self.configs[idx];
        let parts: Vec<&str> = config.command.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        let mut cmd = Command::new(parts[0]);
        if parts.len() > 1 {
            cmd.args(&parts[1..]);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        match cmd.spawn() {
            Ok(mut child) => {
                let stdin = child.stdin.take().unwrap();
                let stdout = child.stdout.take().unwrap();

                let event_tx = self.event_tx.clone();
                tokio::spawn(async move {
                    let reader = BufReader::new(stdout);
                    let mut lines = reader.lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        if let Ok(output) = serde_json::from_str::<PluginOutput>(&line) {
                            if !output.segments.is_empty() {
                                let _ = event_tx.send(PluginEvent::SegmentsUpdated {
                                    plugin_idx: idx,
                                    segments: output.segments,
                                });
                            }
                            if !output.commands.is_empty() {
                                let _ = event_tx.send(PluginEvent::Commands {
                                    commands: output.commands,
                                });
                            }
                        }
                    }
                });

                self.children[idx] = Some(PluginChild { child, stdin });

                let interval = self.configs[idx].refresh_interval_secs;
                if interval > 0 {
                    let event_tx = self.event_tx.clone();
                    tokio::spawn(async move {
                        let mut ticker = tokio::time::interval(Duration::from_secs(interval));
                        loop {
                            ticker.tick().await;
                            let _ = event_tx.send(PluginEvent::Commands {
                                commands: vec![],
                            });
                        }
                    });
                }
            }
            Err(e) => {
                eprintln!("pane: failed to start plugin '{}': {}", config.command, e);
            }
        }
    }

    pub async fn send_event(
        &mut self,
        event: &str,
        workspace: Option<&str>,
        stats: Option<&pane_protocol::system_stats::SystemStats>,
    ) {
        let input = PluginInput {
            event: event.to_string(),
            workspace: workspace.map(|s| s.to_string()),
            system_stats: stats.map(|s| PluginStats {
                cpu_percent: s.cpu_percent,
                memory_percent: s.memory_percent,
                load_avg_1: s.load_avg_1,
            }),
        };

        let json = match serde_json::to_string(&input) {
            Ok(j) => j,
            Err(_) => return,
        };

        let event_str = event.to_string();
        let matching: Vec<usize> = self
            .configs
            .iter()
            .enumerate()
            .filter(|(_, config)| {
                config.events.contains(&event_str) || config.events.contains(&"*".to_string())
            })
            .map(|(i, _)| i)
            .collect();

        let mut to_restart = Vec::new();
        for i in &matching {
            if let Some(ref mut pc) = self.children[*i] {
                let line = format!("{}\n", json);
                if timeout(Duration::from_secs(2), pc.stdin.write_all(line.as_bytes()))
                    .await
                    .is_err()
                {
                    eprintln!(
                        "pane: plugin '{}' timed out on write",
                        self.configs[*i].command
                    );
                    let _ = pc.child.kill().await;
                    self.children[*i] = None;
                    to_restart.push(*i);
                }
            }
        }
        for i in to_restart {
            self.start_plugin(i);
        }
    }

    pub fn all_segments(&self) -> Vec<&[PluginSegment]> {
        self.segments.iter().map(|s| s.as_slice()).collect()
    }

    pub fn handle_event(&mut self, event: PluginEvent) -> Vec<String> {
        match event {
            PluginEvent::SegmentsUpdated {
                plugin_idx,
                segments,
            } => {
                if plugin_idx < self.segments.len() {
                    self.segments[plugin_idx] = segments;
                }
                Vec::new()
            }
            PluginEvent::Commands { commands } => commands,
        }
    }
}
