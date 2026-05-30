use anyhow::{Context, Result};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io::Stdout;
use std::path::{Path, PathBuf};

use crate::session::AgentSession;

fn config_dir() -> PathBuf {
    dirs::home_dir()
        .expect("home directory not found")
        .join(".config/agent-dash")
}

pub fn notes_dir() -> PathBuf {
    config_dir().join("notes")
}

fn sanitize_key(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn session_note_path(session: &AgentSession) -> PathBuf {
    let key = match session.session_id.as_deref() {
        Some(id) if !id.is_empty() => sanitize_key(id),
        _ => sanitize_key(&session.pane_id),
    };
    notes_dir().join("sessions").join(format!("{}.md", key))
}

pub fn group_note_path(tmux_session_name: &str) -> PathBuf {
    let key = sanitize_key(tmux_session_name);
    notes_dir().join("groups").join(format!("{}.md", key))
}

/// Read a note's content, returning `None` when the file is missing or has no
/// meaningful content (empty or whitespace-only). An empty note is treated as
/// "no note" so it never triggers the note preview pane.
pub fn read_note(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    if content.trim().is_empty() {
        None
    } else {
        Some(content)
    }
}

/// Suspend the TUI, open the note at `path` in $EDITOR (falling back to $VISUAL,
/// then `vi`), restore the TUI, and propagate any failure.
pub fn open_in_editor(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    path: &Path,
) -> Result<()> {
    // Ensure the note directory exists before opening the editor.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create note directory: {}", parent.display()))?;
    }

    // Suspend the TUI.
    disable_raw_mode().context("failed to disable raw mode")?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .context("failed to leave alternate screen")?;

    // Resolve the editor: prefer $EDITOR, then $VISUAL, then "vi".
    let editor_str = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string());
    let editor_str = editor_str.trim().to_string();
    let editor_str = if editor_str.is_empty() {
        "vi".to_string()
    } else {
        editor_str
    };

    // Split on whitespace so an editor with args (e.g. "code -w") still works.
    let mut parts = editor_str.split_whitespace();
    let prog = parts.next().unwrap_or("vi");
    let leading_args: Vec<&str> = parts.collect();

    let status = std::process::Command::new(prog)
        .args(&leading_args)
        .arg(path)
        .status();

    // Restore the TUI regardless of whether the editor succeeded.
    enable_raw_mode().context("failed to re-enable raw mode")?;
    execute!(
        terminal.backend_mut(),
        EnterAlternateScreen,
        EnableMouseCapture
    )
    .context("failed to re-enter alternate screen")?;
    terminal.clear().context("failed to clear terminal")?;

    // Only now surface any editor failure, with the terminal already restored.
    let exit_status = status.with_context(|| format!("failed to launch editor '{}'", prog))?;
    if !exit_status.success() {
        anyhow::bail!("editor '{}' exited with status: {}", prog, exit_status);
    }

    Ok(())
}
