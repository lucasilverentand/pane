use crossterm::{
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
    event::{EnableMouseCapture, DisableMouseCapture},
};
use ratatui::{backend::CrosstermBackend, Frame, Terminal};
use std::io::{self, Stdout};

pub struct Tui {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    entered: bool,
}

impl Tui {
    pub fn new() -> anyhow::Result<Self> {
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;
        Ok(Self {
            terminal,
            entered: false,
        })
    }

    pub fn enter(&mut self) -> anyhow::Result<()> {
        terminal::enable_raw_mode()?;
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableMouseCapture,
        )?;
        self.terminal.clear()?;
        self.entered = true;
        Ok(())
    }

    pub fn exit(&mut self) {
        if self.entered {
            self.entered = false;
            let _ = self.terminal.show_cursor();
            let _ = execute!(
                io::stdout(),
                DisableMouseCapture,
                LeaveAlternateScreen,
            );
            let _ = terminal::disable_raw_mode();
        }
    }

    pub fn draw(&mut self, f: impl FnOnce(&mut Frame)) -> anyhow::Result<()> {
        self.terminal.draw(f)?;
        Ok(())
    }

    pub fn size(&self) -> anyhow::Result<ratatui::layout::Rect> {
        let size = self.terminal.size()?;
        Ok(ratatui::layout::Rect::new(0, 0, size.width, size.height))
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        self.exit();
    }
}

/// Install a panic hook that restores the terminal before printing the panic.
pub fn install_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            DisableMouseCapture,
            LeaveAlternateScreen,
        );
        original_hook(panic_info);
    }));
}
