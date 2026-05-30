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

use ui::theme::{Palette, ThemeMode};

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
    /// Remap the `q` key to run this shell command instead of quitting.
    /// When set, `q` runs the command and keeps agent-dash open; use Ctrl-C to quit.
    #[arg(long, value_name = "COMMAND")]
    map_q: Option<String>,
    #[command(subcommand)]
    command: Option<Command>,
}

fn resolve_palette(config: &config::AppConfig) -> Palette {
    // 1. Env override
    if let Ok(val) = std::env::var("AGENT_DASH_THEME") {
        let mode = match val.to_lowercase().as_str() {
            "light" => ThemeMode::Light,
            _ => ThemeMode::Dark,
        };
        return Palette::for_mode(mode);
    }

    // 2. Config file preference
    if config.theme != ThemeMode::Dark {
        return Palette::for_mode(config.theme);
    }

    // 3. Default
    Palette::dark()
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

    let config = config::load_config(cli.exit);
    let palette = resolve_palette(&config);

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
        cli.map_q,
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
