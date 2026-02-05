mod app;
mod config;
mod event;
mod layout;
mod pane;
mod session;
mod system_stats;
mod tui;
mod ui;
mod workspace;

use app::{App, CliArgs};
use config::Config;

fn main() -> anyhow::Result<()> {
    tui::install_panic_hook();
    let config = Config::load();
    let args = CliArgs::parse();
    App::run_with_args(args, config)
}
