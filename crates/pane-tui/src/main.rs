mod client;
mod clipboard;
mod copy_mode;
mod event;
mod tui;
mod ui;
mod window;

use clap::{Parser, Subcommand};
use pane_protocol::config::Config;

#[derive(Parser)]
#[command(name = "pane", about = "A TUI terminal multiplexer")]
struct Cli {
    /// Start daemon in background without attaching a client
    #[arg(short, long)]
    detach: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Kill the running daemon
    Kill,
    /// Send keys to a pane
    SendKeys {
        /// Target pane
        #[arg(short = 't', long)]
        target: Option<String>,
        /// Keys to send
        keys: String,
    },
    /// Run the daemon in the foreground (for debugging or manual use)
    Daemon,
    /// tmux compatibility shim — accepts raw tmux CLI syntax
    #[command(hide = true)]
    Tmux {
        /// All remaining arguments (passed through as raw tmux args)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

/// Start the daemon, connect the TUI client, and retry once if the daemon
/// crashes during handshake.
fn start_and_connect(config: Config) -> anyhow::Result<()> {
    pane_daemon::server::daemon::start_daemon()?;
    tui::install_panic_hook();
    let rt = tokio::runtime::Runtime::new()?;

    match rt.block_on(client::Client::run(config.clone())) {
        Ok(()) => Ok(()),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("timed out")
                || msg.contains("connection closed")
                || msg.contains("Connection refused")
                || msg.contains("Broken pipe")
            {
                // Daemon likely crashed — kill stale process and retry once
                eprintln!("pane: daemon connection failed ({}), retrying...", msg);
                pane_daemon::server::daemon::kill_daemon();
                pane_daemon::server::daemon::start_daemon()?;
                rt.block_on(client::Client::run(config))
            } else {
                Err(e)
            }
        }
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::load();

    match cli.command {
        None => {
            if cli.detach {
                pane_daemon::server::daemon::start_daemon()?;
                println!("pane: daemon started");
                Ok(())
            } else {
                start_and_connect(config)
            }
        }
        Some(Commands::Daemon) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(pane_daemon::server::daemon::run_server(config))
        }
        Some(Commands::Kill) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(pane_daemon::server::daemon::kill_session())
        }
        Some(Commands::SendKeys { keys, .. }) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(pane_daemon::server::daemon::send_keys(&keys))
        }
        Some(Commands::Tmux { args }) => pane_daemon::server::tmux_shim::handle_tmux_args(args),
    }
}
