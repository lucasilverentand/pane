mod app;
mod client;
mod clipboard;
mod config;
mod copy_mode;
mod default_keys;
mod event;
mod keys;
mod layout;
#[allow(dead_code)]
mod plugin;
mod server;
mod session;
mod system_stats;
mod tui;
mod ui;
mod window;
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
    /// List workspaces
    Ls,
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

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::load();

    match cli.command {
        None => {
            // Default: auto-start daemon + connect client
            server::daemon::start_daemon()?;
            tui::install_panic_hook();
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(client::Client::run(config))
        }
        Some(Commands::Daemon) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(server::daemon::run_server(config))
        }
        Some(Commands::Ls) => {
            if server::daemon::is_running() {
                // TODO: query daemon for workspace list
                println!("pane daemon is running");
            } else {
                // Show saved state info
                if let Some(saved) = session::store::load() {
                    for ws in &saved.workspaces {
                        let pane_count: usize = ws.groups.iter().map(|g| g.tabs.len()).sum();
                        println!("{}: {} panes", ws.name, pane_count);
                    }
                } else {
                    println!("no saved state");
                }
            }
            Ok(())
        }
        Some(Commands::Kill) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(server::daemon::kill_daemon())
        }
        Some(Commands::SendKeys { keys, .. }) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(server::daemon::send_keys(&keys))
        }
        Some(Commands::Tmux { args }) => server::tmux_shim::handle_tmux_args(args),
    }
}
