use anyhow::Result;
use crossterm::event::{Event, EventStream};
use futures::StreamExt;
use ratatui::prelude::*;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::mpsc;
use tokio::sync::watch;

use crate::claude_sessions::FsProbe;
use crate::claude_transcript::LastPromptCache;
use crate::config::{AppConfig, PreviewScrollMode};
use crate::copy_mode;
use crate::multiplexer::tmux::TmuxClient;
use crate::multiplexer::PaneRef;
use crate::resize_pane;
use crate::selection::PreviewSelection;
use crate::sessions::live::{LiveSet, Mutation};
use crate::sessions::record::{default_store_path, Store};
use crate::sessions::repo::RepoRoots;
use crate::sessions::view::{self, VisibleItem};
use crate::sessions::{Session, SessionBook};
use crate::state::{self, UiState};
use crate::ui;
use crate::ui::theme::{resolve_palette, Palette};
use crate::worktree::WorktreeRequest;
use actions::process_action;
use keys::handle_key_event;
use mouse::handle_mouse_event;

mod actions;
pub mod import_picker;
mod keys;
mod mouse;
pub mod new_session;
mod poll;
mod undo;
pub mod view_config;

pub enum Focus {
    Sessions,
    Preview,
}

#[derive(Clone)]
pub struct PreviewTarget {
    pub pane_target: String,
}

pub struct AppState {
    pub should_quit: bool,
    pub config: AppConfig,
    pub book: SessionBook,
    pub ui: UiState,
    pub visible_items: Vec<VisibleItem>,
    /// What `visible_items` was last built from, so typing in the filter doesn't clone every
    /// record again. Only `refresh_visible_items` replaces it.
    sessions_snapshot: Vec<Session>,
    repo_roots: RepoRoots,
    pub selected_index: usize,
    pub focus: Focus,
    pub preview_content: String,
    pub preview_scroll_offset: u16,
    pub preview_is_sticky_bottom: bool,
    pub preview_content_height: u16,
    pub preview_area_height: u16,
    pub preview_pane_area: Rect,
    pub preview_selection: Option<PreviewSelection>,
    pub copy_mode: Option<copy_mode::CopyModeState>,
    pub pending_confirm_target: Option<PaneRef>,
    pub show_help: bool,
    pub view_config: Option<view_config::ViewConfigOverlay>,
    pub new_session: Option<new_session::NewSessionDialog>,
    /// Lets actions hand slow work to a task that reports back through the event loop.
    message_tx: mpsc::UnboundedSender<Message>,
    pub sessions_expanded: bool,
    undo: undo::UndoStack,
    last_prompts: LastPromptCache,
    /// `last-prompt` of the selected session when it isn't live; refreshed on selection and
    /// discovery ticks rather than per frame.
    pub closed_session_prompt: Option<String>,

    pub help_filter_active: bool,
    pub help_filter_query: String,
    pub help_filter_cursor: usize,

    pub session_filter_active: bool,
    pub session_filter_query: String,
    pub session_filter_cursor: usize,
    pub toast_message: Option<String>,
    pub toast_deadline: Option<std::time::Instant>,
    pub initial_focused_info: Option<(String, String)>,
    pub theme: Palette,
    /// When set, the exit action runs this shell command instead of quitting.
    pub map_exit: Option<String>,
    /// Holds a pending exit command to be spawned after the current action completes.
    /// Set by `request_exit` when `map_exit` is configured; drained in the event loop.
    pub pending_exit_cmd: Option<String>,
    /// When true, the resize task is paused and windows are restored.
    pub resize_paused: bool,
    /// When set, the run loop will open this note path in $EDITOR after the current event.
    pub pending_note_edit: Option<std::path::PathBuf>,
}

pub enum Message {
    /// `None` when the multiplexer couldn't be reached.
    SessionsUpdated(Option<LiveSet>),
    /// The first failure of a streak of failed discovery polls.
    TmuxUnreachable(String),
    PreviewUpdated(String),
    ConfigChanged,
    /// A worktree command run finished; `Ok` holds the new worktree's path.
    WorktreeCreated {
        name: String,
        result: Result<String>,
    },
}

pub enum Action {
    SwitchToPane(PaneRef),
    OpenPopup(String),
    ResumeSession {
        session_id: String,
        /// Names the tmux session.
        label: String,
        cwd: String,
    },
    CreateSession {
        cwd: String,
        /// When set, `cwd` is the repository the worktree is created from.
        worktree: Option<WorktreeRequest>,
    },
    /// Starts the session in a worktree that `CreateSession` created.
    SpawnInWorktree {
        path: String,
        name: String,
    },
    KillPane(PaneRef),
    ForwardScrollDown {
        target: String,
        col: u16,
        row: u16,
    },
    ForwardScrollUp {
        target: String,
        col: u16,
        row: u16,
    },
}

pub async fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    exit_on_switch: bool,
    exit_immediately: bool,
    map_exit: Option<String>,
    no_auto_focus: bool,
    palette: Palette,
) -> Result<()> {
    let config = crate::config::load_config(exit_on_switch);
    let ui_state = state::load_state(config.shared_state)?;
    let store_path = default_store_path();
    let store = match Store::load(store_path.clone())? {
        Some(store) => store,
        None => Store::empty(store_path),
    };

    let focused_pane_info = if no_auto_focus {
        None
    } else {
        let tmux = TmuxClient;
        tmux.get_focused_pane_info().await
    };

    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    let mut state = AppState {
        should_quit: false,
        config,
        book: SessionBook::new(store),
        ui: ui_state,
        visible_items: Vec::new(),
        sessions_snapshot: Vec::new(),
        repo_roots: RepoRoots::default(),
        selected_index: 0,
        focus: Focus::Sessions,
        preview_content: String::new(),
        preview_scroll_offset: 0,
        preview_is_sticky_bottom: true,
        preview_content_height: 0,
        preview_area_height: 0,
        preview_pane_area: Rect::default(),
        preview_selection: None,
        copy_mode: None,
        pending_confirm_target: None,
        show_help: false,
        view_config: None,
        new_session: None,
        message_tx: tx.clone(),
        sessions_expanded: false,
        undo: undo::UndoStack::default(),
        last_prompts: LastPromptCache::default(),
        closed_session_prompt: None,

        help_filter_active: false,
        help_filter_query: String::new(),
        help_filter_cursor: 0,

        session_filter_active: false,
        session_filter_query: String::new(),
        session_filter_cursor: 0,
        toast_message: None,
        toast_deadline: None,
        initial_focused_info: focused_pane_info,
        theme: palette,
        map_exit,
        pending_exit_cmd: None,
        resize_paused: false,
        pending_note_edit: None,
    };

    // Persisted sessions show (as closed) before the first discovery tick lands.
    refresh_visible_items(&mut state);

    let _config_watcher = match crate::config_watch::spawn(tx.clone()) {
        Ok(watcher) => Some(watcher),
        Err(err) => {
            eprintln!(
                "agent-dash: failed to watch config file {}: {}",
                crate::config::config_location().display(),
                err
            );
            None
        }
    };

    let (target_tx, target_rx) = watch::channel(Option::<PreviewTarget>::None);

    poll::spawn(tx.clone());

    // Preview task — pipe-pane notification with fallback polling
    let mut pipe_watcher = crate::pipe_pane::PipePaneWatcher::new();
    let fifo_path = pipe_watcher.fifo_path().to_string();
    crate::pipe_pane::spawn_preview_task(tx.clone(), target_rx, fifo_path);

    let (resize_tx, resize_rx) =
        watch::channel::<resize_pane::ResizeCommand>(resize_pane::ResizeCommand::Apply(None));
    let resize_handle = resize_pane::spawn_resize_task(resize_rx);

    let mut event_stream = EventStream::new();

    // Initial render
    terminal.draw(|frame| ui::render(frame, &mut state))?;

    if exit_immediately {
        return Ok(());
    }

    let mut resize_restore_sent = false;

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sighup = signal(SignalKind::hangup())?;

    loop {
        tokio::select! {
            Some(Ok(event)) = event_stream.next() => {
                match event {
                    Event::Key(key) => {
                        state.resize_paused = false;
                        let action = handle_key_event(&mut state, key, &target_tx);
                        if let Some(action) = action {
                            process_action(&mut state, action).await;
                        }
                    }
                    Event::Mouse(mouse) => {
                        state.resize_paused = false;
                        if let Some(action) = handle_mouse_event(&mut state, mouse) {
                            process_action(&mut state, action).await;
                        }
                    }
                    // On terminal resize (including abduco reattach, which re-applies the
                    // pty size and fires SIGWINCH), the physical screen is wiped but
                    // ratatui's diff renderer still believes the old frame is on screen.
                    // Clear to reset the buffer so the next draw repaints every cell.
                    Event::Resize(_, _) => {
                        terminal.clear()?;
                    }
                    _ => {}
                }
            }
            Some(msg) = rx.recv() => {
                if let Some(action) = handle_message(&mut state, msg, &target_tx) {
                    process_action(&mut state, action).await;
                }
            }
            _ = sigterm.recv() => {
                state.should_quit = true;
            }
            _ = sigint.recv() => {
                state.should_quit = true;
            }
            _ = sighup.recv() => {
                state.should_quit = true;
            }
        }

        terminal.draw(|frame| ui::render(frame, &mut state))?;

        let exit_cmd = state.pending_exit_cmd.take();
        if exit_cmd.is_some() {
            state.resize_paused = true;
            resize_restore_sent = false;
        }

        if state.resize_paused {
            if !resize_restore_sent {
                let _ = resize_tx.send(resize_pane::ResizeCommand::Restore {
                    then_exec: exit_cmd,
                });
                resize_restore_sent = true;
            }
        } else {
            resize_restore_sent = false;
            let _ = resize_tx.send(resize_pane::ResizeCommand::Apply(build_resize_request(
                &state,
            )));
        }

        // Check toast expiry
        if let Some(deadline) = state.toast_deadline {
            if std::time::Instant::now() >= deadline {
                state.toast_message = None;
                state.toast_deadline = None;
            }
        }

        if let Some(path) = state.pending_note_edit.take() {
            if let Err(err) = crate::notes::open_in_editor(terminal, &path) {
                state.toast_message = Some(format!("Note error: {}", err));
                state.toast_deadline =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(4));
            }
        }

        if state.should_quit {
            break;
        }
    }

    pipe_watcher.cleanup();

    drop(resize_tx);
    let _ = resize_handle.await;

    state.book.flush(now_ms())?;
    Ok(())
}

fn handle_message(
    state: &mut AppState,
    msg: Message,
    selected_pane_target: &watch::Sender<Option<PreviewTarget>>,
) -> Option<Action> {
    match msg {
        Message::SessionsUpdated(live) => {
            let old_items = std::mem::take(&mut state.visible_items);
            state.book.set_auto_track(state.config.auto_track_sessions);
            if let Err(err) = state.book.update(live, &FsProbe, now_ms()) {
                show_toast(state, format!("Session store error: {err:#}"), 4);
            }
            refresh_visible_items(state);
            if let Some(info) = state.initial_focused_info.take() {
                state.selected_index =
                    view::auto_select_index(&state.visible_items, &info.0, &info.1);
            } else {
                state.selected_index = view::resolve_selected_index(
                    &state.visible_items,
                    &old_items,
                    state.selected_index,
                );
            }

            update_selected_target(state, selected_pane_target);
        }
        Message::TmuxUnreachable(err) => {
            show_toast(state, format!("tmux unreachable: {err}"), 4);
        }
        Message::PreviewUpdated(content) => {
            if state.copy_mode.is_some() {
                return None;
            }
            if !state
                .preview_selection
                .as_ref()
                .is_some_and(|s| s.is_dragging)
            {
                state.preview_selection = None;
            }
            state.preview_content = content;
        }
        Message::ConfigChanged => match crate::config::try_load_config(state.config.exit_on_switch)
        {
            Ok(Some(config)) => {
                state.theme = resolve_palette(config.theme);
                state.config = config;
            }
            Ok(None) => {}
            Err(err) => {
                eprintln!(
                    "agent-dash: failed to reload config from {}: {}",
                    crate::config::config_location().display(),
                    err
                );
            }
        },
        Message::WorktreeCreated { name, result } => {
            // Only clears this run's own toast, in case another run's is showing.
            if state.toast_message.as_deref() == Some(&creating_worktree_toast(&name)) {
                state.toast_message = None;
                state.toast_deadline = None;
            }
            match result {
                Ok(path) => return Some(Action::SpawnInWorktree { path, name }),
                Err(err) => show_toast(state, format!("{err:#}"), 6),
            }
        }
    }
    None
}

fn creating_worktree_toast(name: &str) -> String {
    format!("Creating worktree {name}…")
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_millis() as u64
}

pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_secs()
}

fn show_toast(state: &mut AppState, message: impl Into<String>, seconds: u64) {
    state.toast_message = Some(message.into());
    state.toast_deadline =
        Some(std::time::Instant::now() + std::time::Duration::from_secs(seconds));
}

/// Re-reads the sessions from the book. Call it after anything that changed the book.
fn refresh_visible_items(state: &mut AppState) {
    state.sessions_snapshot = state.book.sessions();
    if store_missing_repo_roots(state) {
        state.sessions_snapshot = state.book.sessions();
    }
    refilter_visible_items(state);
}

/// Records the repo root of every session that lacks one while its cwd still exists; Folder
/// grouping reads it from the record, so it has to be saved before a worktree is deleted.
/// Returns whether any record changed.
fn store_missing_repo_roots(state: &mut AppState) -> bool {
    state.repo_roots.resolve_all(
        state
            .sessions_snapshot
            .iter()
            .filter(|session| session.record.repo_root.is_none())
            .map(|session| session.record.cwd.as_str()),
    );
    let mutations: Vec<Mutation> = state
        .sessions_snapshot
        .iter()
        .filter(|session| session.record.repo_root.is_none())
        .filter_map(|session| {
            let repo_root = state.repo_roots.get(&session.record.cwd)?;
            Some(Mutation::SetRepoRoot {
                session_id: session.record.session_id.clone(),
                repo_root: repo_root.to_string(),
            })
        })
        .collect();
    if mutations.is_empty() {
        return false;
    }
    mutate_store(state, mutations);
    true
}

/// Rebuilds the list from the last snapshot; for changes that don't touch the book.
fn refilter_visible_items(state: &mut AppState) {
    let now = now_secs();
    let query = state.session_filter_query.trim();
    state.visible_items = if query.is_empty() {
        view::build_items(
            &state.sessions_snapshot,
            &state.ui.view,
            &state.ui.collapsed_groups,
            now,
        )
    } else {
        view::search_items(&state.sessions_snapshot, &state.ui.view, query, now)
    };
}

fn selected_session(state: &AppState) -> Option<&Session> {
    state
        .visible_items
        .get(state.selected_index)
        .and_then(VisibleItem::session)
}

fn get_selected_pane_target(state: &AppState) -> Option<String> {
    selected_session(state)
        .and_then(Session::pane)
        .map(|pane| pane.target.clone())
}

fn refresh_keeping_selection(state: &mut AppState) {
    let old_items = std::mem::take(&mut state.visible_items);
    refresh_visible_items(state);
    state.selected_index =
        view::resolve_selected_index(&state.visible_items, &old_items, state.selected_index);
}

fn save_ui_state(state: &mut AppState) {
    if let Err(err) = state::save_state(&state.ui, state.config.shared_state) {
        show_toast(state, format!("Failed to save state: {err:#}"), 4);
    }
}

fn mutate_store(state: &mut AppState, mutations: Vec<Mutation>) {
    if let Err(err) = state.book.mutate(mutations, now_ms()) {
        show_toast(state, format!("Session store error: {err:#}"), 4);
    }
}

fn forward_scroll_target(state: &AppState, col: u16, row: u16) -> Option<(String, u16, u16)> {
    if !matches!(
        state.config.claude_code_preview_scroll_mode,
        PreviewScrollMode::Virtualized
    ) {
        return None;
    }
    let target = get_selected_pane_target(state)?;
    Some((target, col, row))
}

fn update_selected_target(
    state: &mut AppState,
    selected_pane_target: &watch::Sender<Option<PreviewTarget>>,
) {
    let closed = selected_session(state)
        .filter(|session| !session.is_live())
        .map(|session| {
            (
                session.record.session_id.clone(),
                session.record.cwd.clone(),
            )
        });
    state.closed_session_prompt =
        closed.and_then(|(session_id, cwd)| state.last_prompts.get(&session_id, &cwd));

    let value = get_selected_pane_target(state).map(|pane_target| PreviewTarget { pane_target });
    let _ = selected_pane_target.send(value);
}

fn scroll_preview_down(state: &mut AppState) {
    let visible_height = state.preview_area_height.saturating_sub(2);
    state.preview_scroll_offset = state.preview_scroll_offset.saturating_add(1);
    if state.preview_scroll_offset >= state.preview_content_height.saturating_sub(visible_height) {
        state.preview_is_sticky_bottom = true;
    }
}

fn scroll_preview_up(state: &mut AppState) {
    if state.preview_scroll_offset > 0 {
        state.preview_scroll_offset -= 1;
        state.preview_is_sticky_bottom = false;
    }
}

fn build_resize_request(state: &AppState) -> Option<resize_pane::ResizeRequest> {
    let pane_target = get_selected_pane_target(state)?;
    let cols = state.preview_pane_area.width.saturating_sub(2);
    let rows = state.preview_pane_area.height.saturating_sub(2);
    Some(resize_pane::ResizeRequest {
        pane_target,
        cols,
        rows,
    })
}
