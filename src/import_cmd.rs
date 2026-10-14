use anyhow::Result;
use clap::{Args, ValueEnum};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::prelude::*;
use std::io::{self, Stdout};

use crate::app::import_picker::{ImportPicker, Outcome};
use crate::app::{now_ms, now_secs};
use crate::sessions::import::{self, ImportCandidate, ImportRoots, ImportSource, ScanOutcome};
use crate::sessions::live::{self, Mutation};
use crate::sessions::record::{default_store_path, SessionRecord, Store};
use crate::ui;
use crate::ui::theme::Palette;

const DAY_SECS: u64 = 24 * 60 * 60;

#[derive(Clone, Copy, ValueEnum)]
enum Window {
    #[value(name = "1d")]
    Day,
    #[value(name = "3d")]
    ThreeDays,
    #[value(name = "7d")]
    Week,
    #[value(name = "30d")]
    Month,
}

impl Window {
    fn secs(self) -> u64 {
        match self {
            Window::Day => DAY_SECS,
            Window::ThreeDays => 3 * DAY_SECS,
            Window::Week => 7 * DAY_SECS,
            Window::Month => 30 * DAY_SECS,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum SourceArg {
    Cli,
    Desktop,
}

impl SourceArg {
    fn source(self) -> ImportSource {
        match self {
            SourceArg::Cli => ImportSource::Cli,
            SourceArg::Desktop => ImportSource::Desktop,
        }
    }
}

/// Without `--all` or `--since`, opens a picker.
#[derive(Args)]
pub struct ImportArgs {
    /// Import every session not tracked yet, without a picker
    #[arg(long, conflicts_with = "since", group = "selection")]
    all: bool,
    /// Import the untracked sessions active within this window, without a picker
    #[arg(long, value_name = "WINDOW", group = "selection")]
    since: Option<Window>,
    /// With --all or --since, import only sessions from this source
    #[arg(long, value_enum, requires = "selection")]
    source: Option<SourceArg>,
}

pub fn run(args: ImportArgs, palette: Palette) -> Result<()> {
    let store_path = default_store_path();
    let mut store = match Store::load(store_path.clone())? {
        Some(store) => store,
        None => Store::empty(store_path),
    };
    let known = store
        .records()
        .map(|record| record.session_id.clone())
        .collect();
    let roots = ImportRoots::from_home();

    let chosen = if args.all || args.since.is_some() {
        let outcome = import::scan(&roots, &known)?;
        report_skipped(&outcome);
        let source = args.source.map(SourceArg::source);
        let cutoff = args
            .since
            .map(|window| now_secs().saturating_sub(window.secs()));
        outcome
            .candidates
            .into_iter()
            .filter(|candidate| cutoff.is_none_or(|cutoff| candidate.last_activity >= cutoff))
            .filter(|candidate| source.is_none_or(|source| candidate.source == source))
            .collect()
    } else {
        pick(&roots, &known, palette)?
    };

    let count = chosen.len();
    if count > 0 {
        store_candidates(&mut store, chosen)?;
    }
    let noun = if count == 1 { "session" } else { "sessions" };
    println!("Imported {count} {noun}");
    Ok(())
}

fn report_skipped(outcome: &ScanOutcome) {
    if outcome.skipped > 0 {
        eprintln!("agent-dash: skipped {} unreadable files", outcome.skipped);
    }
}

fn store_candidates(store: &mut Store, candidates: Vec<ImportCandidate>) -> Result<()> {
    let now = now_ms();
    let mutations = candidates
        .into_iter()
        .map(|candidate| {
            Mutation::Insert(SessionRecord::new(
                candidate.session_id,
                candidate.cwd,
                candidate.title,
                candidate.last_activity,
                candidate.last_activity,
                candidate.source.origin(),
                now,
            ))
        })
        .collect();
    live::apply(store, mutations, now);
    store.save(now)
}

/// Puts the terminal back when dropped, so every exit path restores it.
struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn enter() -> Result<TerminalGuard> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        Ok(TerminalGuard {
            terminal: Terminal::new(CrosstermBackend::new(stdout))?,
        })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

fn pick(
    roots: &ImportRoots,
    known: &std::collections::HashSet<String>,
    palette: Palette,
) -> Result<Vec<ImportCandidate>> {
    let mut guard = TerminalGuard::enter()?;
    let terminal = &mut guard.terminal;

    terminal.draw(|frame| ui::import_picker::render_scanning(frame, &palette))?;
    let mut picker = ImportPicker::new(import::scan(roots, known)?);

    loop {
        terminal.draw(|frame| ui::import_picker::render(frame, &palette, &picker))?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        // Raw mode turns Ctrl+C into a key event instead of SIGINT.
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
            return Ok(Vec::new());
        }
        match picker.handle_key(key) {
            Outcome::Pending => {}
            Outcome::Closed => return Ok(Vec::new()),
            Outcome::Import(chosen) => return Ok(chosen),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::import::ImportSource;
    use crate::sessions::record::tests::TempDir;

    #[derive(clap::Parser)]
    struct Cli {
        #[command(flatten)]
        args: ImportArgs,
    }

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        use clap::Parser;
        Cli::try_parse_from(std::iter::once("import").chain(args.iter().copied()))
    }

    #[test]
    fn source_combines_with_all_and_since_but_not_with_the_picker() {
        assert!(parse(&["--all", "--source", "cli"]).is_ok());
        assert!(parse(&["--since", "7d", "--source", "desktop"]).is_ok());
        assert!(parse(&["--source", "cli"]).is_err());
        assert!(parse(&["--all", "--since", "1d"]).is_err());
        assert!(parse(&["--all", "--source", "other"]).is_err());
        assert!(parse(&["--since", "2d"]).is_err());
    }

    #[test]
    fn imported_candidates_are_saved_with_their_source_as_origin() {
        let dir = TempDir::new("import-cmd");
        let mut store = Store::empty(dir.store_path());
        store_candidates(
            &mut store,
            vec![ImportCandidate {
                session_id: "00000000-0000-4000-8000-000000000001".to_string(),
                source: ImportSource::Desktop,
                title: "T".to_string(),
                cwd: "/work/app".to_string(),
                last_activity: 42,
            }],
        )
        .unwrap();

        let reloaded = Store::load(dir.store_path()).unwrap().unwrap();
        let record = reloaded
            .get("00000000-0000-4000-8000-000000000001")
            .unwrap();
        assert_eq!(record.last_activity, 42);
        assert_eq!(record.origin, ImportSource::Desktop.origin());
    }
}
