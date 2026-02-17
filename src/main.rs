mod app;
mod client;
mod clipboard;
mod config;
mod copy_mode;
mod event;
mod keys;
mod layout;
#[allow(dead_code)]
mod layout_presets;
mod pane;
mod server;
mod session;
mod system_stats;
mod tui;
mod ui;
mod workspace;

use clap::{Parser, Subcommand};
use config::Config;

#[derive(Parser)]
#[command(name = "pane", about = "A TUI terminal multiplexer")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new session
    New {
        /// Session name
        name: Option<String>,
    },
    /// Attach to an existing session
    Attach {
        /// Session name
        name: Option<String>,
    },
    /// List running sessions
    Ls,
    /// Kill a session
    KillSession {
        /// Session name
        name: Option<String>,
    },
    /// Send keys to a pane
    SendKeys {
        /// Target pane (session:window.pane)
        #[arg(short = 't', long)]
        target: Option<String>,
        /// Keys to send
        keys: String,
    },
    /// Run the daemon in the foreground (for debugging or manual use)
    Daemon {
        /// Session name
        name: Option<String>,
    },
    /// tmux compatibility shim — accepts raw tmux CLI syntax
    #[command(hide = true)]
    Tmux {
        /// All remaining arguments (passed through as raw tmux args)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::load();

    match cli.command {
        None => {
            // Default: auto-start daemon for "default" session + connect client
            let session_name = "default";
            server::daemon::start_daemon(session_name)?;
            tui::install_panic_hook();
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(client::Client::run(session_name, config))
        }
        Some(Commands::New { name }) => {
            let name = name.unwrap_or_else(|| {
                format!("session-{}", chrono::Utc::now().timestamp())
            });
            server::daemon::start_daemon(&name)?;
            tui::install_panic_hook();
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(client::Client::run(&name, config))
        }
        Some(Commands::Attach { name }) => {
            let name = name.unwrap_or_else(|| "default".to_string());
            // For attach, start daemon if not running (restores from saved session)
            server::daemon::start_daemon(&name)?;
            tui::install_panic_hook();
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(client::Client::run(&name, config))
        }
        Some(Commands::Daemon { name }) => {
            let name = name.unwrap_or_else(|| "default".to_string());
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(server::daemon::run_server(name, config))
        }
        Some(Commands::Ls) => {
            // Check for running server sessions
            let running = server::daemon::list_sessions();
            if running.is_empty() {
                // Fall back to saved sessions
                let saved = session::store::list().unwrap_or_default();
                if saved.is_empty() {
                    println!("no sessions");
                } else {
                    for s in &saved {
                        println!(
                            "{}: {} panes (saved {})",
                            s.name,
                            s.pane_count,
                            s.updated_at.format("%Y-%m-%d %H:%M")
                        );
                    }
                }
            } else {
                for name in &running {
                    println!("{} (running)", name);
                }
                // Also show saved sessions not currently running
                let saved = session::store::list().unwrap_or_default();
                for s in &saved {
                    if !running.contains(&s.name) {
                        println!(
                            "{}: {} panes (saved {})",
                            s.name,
                            s.pane_count,
                            s.updated_at.format("%Y-%m-%d %H:%M")
                        );
                    }
                }
            }
            Ok(())
        }
        Some(Commands::KillSession { name }) => {
            let name = name.unwrap_or_else(|| "default".to_string());
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(server::daemon::kill_session(&name))
        }
        Some(Commands::SendKeys { target, keys }) => {
            let session_name = target
                .as_deref()
                .unwrap_or("default")
                .split(':')
                .next()
                .unwrap_or("default")
                .to_string();
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(server::daemon::send_keys(&session_name, &keys))
        }
        Some(Commands::Tmux { args }) => server::tmux_shim::handle_tmux_args(args),
    }
}
