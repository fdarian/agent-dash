use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io;

mod app;
mod cache;
mod claude_sessions;
mod claude_transcript;
mod config;
mod config_watch;
mod copy_mode;
mod enrichment;
mod filter_query;
mod selection;
mod session;
mod state;
mod time_fmt;
mod ui;

mod notes;
mod pipe_pane;
mod resize_pane;
mod tmux;

use ui::theme::resolve_palette;

#[derive(Parser)]
#[command(name = "agent-dash", version)]
struct Cli {
    #[arg(long, default_value_t = false)]
    exit: bool,
    #[arg(long)]
    exit_immediately: bool,
    /// Remap the exit action to run this shell command instead of quitting.
    /// When set, any exit path runs the command and keeps agent-dash open; use Ctrl-C to quit.
    #[arg(long, value_name = "COMMAND")]
    map_exit: Option<String>,
    #[arg(long)]
    no_auto_focus: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let config = config::load_config(cli.exit);
    let palette = resolve_palette(config.theme);

    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run app
    let result = app::run(
        &mut terminal,
        cli.exit,
        cli.exit_immediately,
        cli.map_exit,
        cli.no_auto_focus,
        palette,
    )
    .await;

    // Teardown
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}
