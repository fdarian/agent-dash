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
mod config;
mod copy_mode;
mod enrichment;
mod filter_query;
mod hook_write;
mod selection;
mod session;
mod state;
mod ui;

mod pipe_pane;
mod resize_pane;
mod tmux;

#[derive(clap::Subcommand)]
enum Command {
    /// Write a per-pane enrichment file from a Claude Code hook event.
    ///
    /// Reads JSON from stdin and $TMUX_PANE from env. When $TMUX_PANE is unset
    /// (Claude launched outside tmux) this is a silent no-op.
    HookWrite {
        /// Hook event name: session-start, prompt-submit, stop, session-end
        event: String,
    },
}

#[derive(Parser)]
#[command(name = "agent-dash", version)]
struct Cli {
    #[arg(long, default_value_t = false)]
    exit: bool,
    #[arg(long)]
    exit_immediately: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Some(Command::HookWrite { event }) = cli.command {
        match hook_write::EventKind::from_str(&event) {
            Some(kind) => hook_write::execute(kind),
            None => {
                eprintln!(
                    "agent-dash hook-write: unknown event '{}'. \
                     Expected: session-start, prompt-submit, stop, session-end",
                    event
                );
            }
        }
        return Ok(());
    }

    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run app
    let result = app::run(&mut terminal, cli.exit, cli.exit_immediately).await;

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
