mod app;
mod event;
mod layout;
mod pane;
mod session;
mod tui;
mod ui;
mod workspace;

use app::{App, CliArgs};

fn main() -> anyhow::Result<()> {
    tui::install_panic_hook();
    let args = CliArgs::parse();
    App::run_with_args(args)
}
