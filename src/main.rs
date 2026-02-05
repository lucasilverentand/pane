mod app;
mod clipboard;
mod config;
mod copy_mode;
mod event;
mod layout;
mod layout_presets;
mod pane;
mod server;
mod session;
mod system_stats;
mod tui;
mod ui;
mod workspace;

use app::{App, CliArgs};
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
            // Default: attach to existing or start new (embedded mode)
            tui::install_panic_hook();
            let args = CliArgs::Default;
            App::run_with_args(args, config)
        }
        Some(Commands::New { name }) => {
            tui::install_panic_hook();
            let name = name.unwrap_or_else(|| {
                format!("session-{}", chrono::Utc::now().timestamp())
            });
            let args = CliArgs::New(name);
            App::run_with_args(args, config)
        }
        Some(Commands::Attach { name }) => {
            tui::install_panic_hook();
            let name = name.unwrap_or_else(|| "default".to_string());
            let args = CliArgs::Attach(name);
            App::run_with_args(args, config)
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
