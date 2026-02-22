use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers};
use futures::StreamExt;
use ratatui::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use crate::cache::{load_cached_sessions, save_cached_sessions, CachedSessionData};
use crate::config::AppConfig;
use crate::session::{
    build_visible_items, group_sessions_by_name, resolve_selected_index, ClaudeSession,
    SessionStatus, VisibleItem,
};
use crate::state;
use crate::tmux::TmuxClient;
use crate::ui;

pub enum Focus {
    Sessions,
    Preview,
}

pub struct AppState {
    pub should_quit: bool,
    pub config: AppConfig,
    pub sessions: Vec<ClaudeSession>,
    pub visible_items: Vec<VisibleItem>,
    pub selected_index: usize,
    pub focus: Focus,
    pub collapsed_groups: HashSet<String>,
    pub unread_pane_ids: HashSet<String>,
    pub prev_status_map: HashMap<String, SessionStatus>,
    pub display_name_map: HashMap<String, String>,
    pub preview_content: String,
    pub preview_scroll_offset: u16,
    pub preview_is_sticky_bottom: bool,
    pub preview_content_height: u16,
    pub preview_area_height: u16,
    pub pending_confirm_target: Option<String>,
    pub show_help: bool,
    pub terminal_bg: (u8, u8, u8),
    pub help_filter_active: bool,
    pub help_filter_query: String,
    pub toast_message: Option<String>,
    pub toast_deadline: Option<std::time::Instant>,
}

pub enum Message {
    SessionsUpdated(Vec<ClaudeSession>, HashMap<String, String>),
    PreviewUpdated(String),
}

pub enum Action {
    SwitchToPane(String),
    OpenPopup(String),
    CreateSession { session_name: String, cwd_target: String },
    KillPane(String),
}

pub async fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    exit_on_switch: bool,
    terminal_bg: (u8, u8, u8),
) -> Result<()> {
    let config = crate::config::load_config(exit_on_switch);
    let formatter_path = config.session_name_formatter.clone();
    let loaded_state = state::load_state();

    let mut state = AppState {
        should_quit: false,
        config,
        sessions: Vec::new(),
        visible_items: Vec::new(),
        selected_index: 0,
        focus: Focus::Sessions,
        collapsed_groups: HashSet::new(),
        unread_pane_ids: loaded_state.unread_pane_ids,
        prev_status_map: loaded_state.prev_status_map,
        display_name_map: HashMap::new(),
        preview_content: String::new(),
        preview_scroll_offset: 0,
        preview_is_sticky_bottom: true,
        preview_content_height: 0,
        preview_area_height: 0,
        pending_confirm_target: None,
        show_help: false,
        terminal_bg,
        help_filter_active: false,
        help_filter_query: String::new(),
        toast_message: None,
        toast_deadline: None,
    };

    // Load cached sessions for instant first render
    if let Some(cached) = load_cached_sessions() {
        state.sessions = cached.sessions;
        state.display_name_map = cached.display_names;
        refresh_visible_items(&mut state);
    }

    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    let selected_pane_target = Arc::new(Mutex::new(Option::<String>::None));

    // Session polling task (every 2s)
    let poll_tx = tx.clone();
    tokio::spawn(async move {
        let config = crate::config::load_config(false);
        let tmux = TmuxClient::new(&config);
        let mut formatter_cache: HashMap<String, String> = HashMap::new();
        loop {
            if let Ok(sessions) = tmux.discover_sessions().await {
                let unique_names: Vec<String> = sessions
                    .iter()
                    .map(|s| s.session_name.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();

                let mut display_names = HashMap::new();
                for name in &unique_names {
                    let formatted = if let Some(ref path) = formatter_path {
                        if let Some(cached) = formatter_cache.get(name) {
                            cached.clone()
                        } else {
                            match tokio::process::Command::new(path)
                                .arg(name)
                                .output()
                                .await
                            {
                                Ok(output) if output.status.success() => {
                                    let result =
                                        String::from_utf8_lossy(&output.stdout).trim().to_string();
                                    formatter_cache.insert(name.clone(), result.clone());
                                    result
                                }
                                _ => name.clone(),
                            }
                        }
                    } else {
                        name.clone()
                    };
                    display_names.insert(name.clone(), formatted);
                }

                // Save to cache
                let cached_data = CachedSessionData {
                    sessions: sessions.clone(),
                    display_names: display_names.clone(),
                };
                save_cached_sessions(&cached_data);

                let _ = poll_tx.send(Message::SessionsUpdated(sessions, display_names));
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    });

    // Preview polling task (every 200ms)
    let preview_tx = tx.clone();
    let preview_target = Arc::clone(&selected_pane_target);
    tokio::spawn(async move {
        let config = crate::config::load_config(false);
        let tmux = TmuxClient::new(&config);
        let mut previous_content = String::new();
        loop {
            let target = preview_target.lock().await.clone();
            if let Some(target) = target {
                if let Ok(content) = tmux.capture_pane_content(&target).await {
                    if content != previous_content {
                        previous_content = content.clone();
                        let _ = preview_tx.send(Message::PreviewUpdated(content));
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    });

    let mut event_stream = EventStream::new();

    // Initial render
    terminal.draw(|frame| ui::render(frame, &mut state))?;

    loop {
        tokio::select! {
            Some(Ok(event)) = event_stream.next() => {
                if let Event::Key(key) = event {
                    let action = handle_key_event(&mut state, key, &selected_pane_target);
                    if let Some(action) = action {
                        process_action(&mut state, action, &selected_pane_target).await;
                    }
                }
            }
            Some(msg) = rx.recv() => {
                handle_message(&mut state, msg, &selected_pane_target);
            }
        }

        terminal.draw(|frame| ui::render(frame, &mut state))?;

        // Check toast expiry
        if let Some(deadline) = state.toast_deadline {
            if std::time::Instant::now() >= deadline {
                state.toast_message = None;
                state.toast_deadline = None;
            }
        }

        if state.should_quit {
            break;
        }
    }

    Ok(())
}

async fn process_action(
    state: &mut AppState,
    action: Action,
    selected_pane_target: &Arc<Mutex<Option<String>>>,
) {
    match action {
        Action::SwitchToPane(target) => {
            let config = crate::config::load_config(false);
            let tmux = TmuxClient::new(&config);
            let _ = tmux.switch_to_pane(&target).await;
        }
        Action::OpenPopup(target) => {
            let config = crate::config::load_config(false);
            let tmux = TmuxClient::new(&config);
            let _ = tmux.open_popup(&target).await;
        }
        Action::CreateSession { session_name, cwd_target } => {
            let config = crate::config::load_config(state.config.exit_on_switch);
            let tmux = TmuxClient::new(&config);
            if let Ok(cwd) = tmux.get_pane_cwd(&cwd_target).await {
                if let Ok(Some(pane_info)) = tmux.create_window(&session_name, Some(&cwd)).await {
                    let _ = tmux.switch_to_pane(&pane_info.pane_target).await;
                    if state.config.exit_on_switch {
                        state.should_quit = true;
                    } else {
                        let new_session = ClaudeSession {
                            pane_id: pane_info.pane_id,
                            pane_target: pane_info.pane_target,
                            title: pane_info.pane_title.clone(),
                            session_name: pane_info.session_name.clone(),
                            status: crate::session::parse_session_status(&pane_info.pane_title),
                        };
                        state.prev_status_map.insert(new_session.pane_id.clone(), new_session.status.clone());
                        state.sessions.push(new_session);
                        state::save_state(&state.unread_pane_ids, &state.prev_status_map);
                        let old_items = std::mem::take(&mut state.visible_items);
                        refresh_visible_items(state);
                        state.selected_index = resolve_selected_index(&state.visible_items, &old_items, state.selected_index);
                        update_selected_target(state, selected_pane_target);
                    }
                }
            }
        }
        Action::KillPane(target) => {
            let config = crate::config::load_config(false);
            let tmux = TmuxClient::new(&config);
            let _ = tmux.kill_pane(&target).await;
            if let Some(removed) = state.sessions.iter().find(|s| s.pane_target == target) {
                let pane_id = removed.pane_id.clone();
                state.prev_status_map.remove(&pane_id);
                state.unread_pane_ids.remove(&pane_id);
            }
            state.sessions.retain(|s| s.pane_target != target);
            state::save_state(&state.unread_pane_ids, &state.prev_status_map);
            let old_items = std::mem::take(&mut state.visible_items);
            refresh_visible_items(state);
            state.selected_index = resolve_selected_index(&state.visible_items, &old_items, state.selected_index);
            update_selected_target(state, selected_pane_target);
        }
    }
}

fn handle_message(
    state: &mut AppState,
    msg: Message,
    selected_pane_target: &Arc<Mutex<Option<String>>>,
) {
    match msg {
        Message::SessionsUpdated(sessions, display_names) => {
            // Update unread tracking
            let mut next_unread = state.unread_pane_ids.clone();
            let current_pane_ids: HashSet<String> =
                sessions.iter().map(|s| s.pane_id.clone()).collect();

            for session in &sessions {
                if let Some(prev_status) = state.prev_status_map.get(&session.pane_id) {
                    if *prev_status == SessionStatus::Active
                        && session.status == SessionStatus::Idle
                    {
                        next_unread.insert(session.pane_id.clone());
                    }
                }
            }

            // Remove unread for panes that no longer exist
            next_unread.retain(|id| current_pane_ids.contains(id));

            // Update prev status map
            let mut next_status_map = HashMap::new();
            for session in &sessions {
                next_status_map.insert(session.pane_id.clone(), session.status.clone());
            }

            state.sessions = sessions;
            state.display_name_map = display_names;
            state.prev_status_map = next_status_map;
            state.unread_pane_ids = next_unread;

            // Persist state
            state::save_state(&state.unread_pane_ids, &state.prev_status_map);

            // Resolve selected index
            let old_items = std::mem::take(&mut state.visible_items);
            refresh_visible_items(state);
            state.selected_index =
                resolve_selected_index(&state.visible_items, &old_items, state.selected_index);

            update_selected_target(state, selected_pane_target);
        }
        Message::PreviewUpdated(content) => {
            state.preview_content = content;
        }
    }
}

fn handle_key_event(
    state: &mut AppState,
    key: KeyEvent,
    selected_pane_target: &Arc<Mutex<Option<String>>>,
) -> Option<Action> {
    // Confirm dialog takes priority over all other input
    if state.pending_confirm_target.is_some() {
        match key.code {
            KeyCode::Enter => {
                let target = state.pending_confirm_target.take().unwrap();
                return Some(Action::KillPane(target));
            }
            KeyCode::Esc => {
                state.pending_confirm_target = None;
                return None;
            }
            _ => return None,
        }
    }

    // Help overlay takes priority over main input
    if state.show_help {
        if state.help_filter_active {
            match key.code {
                KeyCode::Esc => {
                    state.help_filter_active = false;
                    state.help_filter_query.clear();
                    return None;
                }
                KeyCode::Backspace => {
                    state.help_filter_query.pop();
                    return None;
                }
                KeyCode::Char(c) => {
                    state.help_filter_query.push(c);
                    return None;
                }
                _ => return None,
            }
        } else {
            match key.code {
                KeyCode::Char('?') | KeyCode::Esc => {
                    state.show_help = false;
                    state.help_filter_active = false;
                    state.help_filter_query.clear();
                    return None;
                }
                KeyCode::Char('/') => {
                    state.help_filter_active = true;
                    return None;
                }
                _ => return None,
            }
        }
    }

    match key.code {
        KeyCode::Char('q') => {
            state.should_quit = true;
            None
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.should_quit = true;
            None
        }
        KeyCode::Char('1') => {
            state.focus = Focus::Sessions;
            None
        }
        KeyCode::Char('0') => {
            state.focus = Focus::Preview;
            None
        }
        KeyCode::Char('j') | KeyCode::Down => {
            match state.focus {
                Focus::Sessions => {
                    if state.selected_index < state.visible_items.len().saturating_sub(1) {
                        state.selected_index += 1;
                        state.preview_content.clear();
                        update_selected_target(state, selected_pane_target);
                    }
                }
                Focus::Preview => {
                    let visible_height = state.preview_area_height.saturating_sub(2);
                    state.preview_scroll_offset = state.preview_scroll_offset.saturating_add(1);
                    if state.preview_scroll_offset >= state.preview_content_height.saturating_sub(visible_height) {
                        state.preview_is_sticky_bottom = true;
                    }
                }
            }
            None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            match state.focus {
                Focus::Sessions => {
                    if state.selected_index > 0 {
                        state.selected_index -= 1;
                        state.preview_content.clear();
                        update_selected_target(state, selected_pane_target);
                    }
                }
                Focus::Preview => {
                    if state.preview_scroll_offset > 0 {
                        state.preview_scroll_offset -= 1;
                        state.preview_is_sticky_bottom = false;
                    }
                }
            }
            None
        }
        KeyCode::Char('h') => {
            if matches!(state.focus, Focus::Sessions) {
                if let Some(item) = state.visible_items.get(state.selected_index).cloned() {
                    match &item {
                        VisibleItem::GroupHeader { session_name, .. } => {
                            state.collapsed_groups.insert(session_name.clone());
                            refresh_visible_items(state);
                        }
                        VisibleItem::Session { group_session_name, .. } => {
                            let group_name = group_session_name.clone();
                            state.collapsed_groups.insert(group_name.clone());
                            refresh_visible_items(state);
                            if let Some(idx) = state.visible_items.iter().position(|i| {
                                matches!(i, VisibleItem::GroupHeader { session_name, .. } if session_name == &group_name)
                            }) {
                                state.selected_index = idx;
                            }
                        }
                    }
                    update_selected_target(state, selected_pane_target);
                }
            }
            None
        }
        KeyCode::Char('l') => {
            if matches!(state.focus, Focus::Sessions) {
                if let Some(VisibleItem::GroupHeader { session_name, .. }) = state.visible_items.get(state.selected_index).cloned().as_ref() {
                    let name = session_name.clone();
                    state.collapsed_groups.remove(&name);
                    refresh_visible_items(state);
                    update_selected_target(state, selected_pane_target);
                }
            }
            None
        }
        KeyCode::Char('O') => {
            get_selected_pane_target(state).map(Action::OpenPopup)
        }
        KeyCode::Char('o') => {
            if matches!(state.focus, Focus::Sessions) {
                if let Some(item) = state.visible_items.get(state.selected_index).cloned() {
                    if let VisibleItem::Session { ref session, .. } = item {
                        state.unread_pane_ids.remove(&session.pane_id);
                        state::save_state(&state.unread_pane_ids, &state.prev_status_map);
                        refresh_visible_items(state);
                    }
                    let target = match &item {
                        VisibleItem::Session { session, .. } => Some(session.pane_target.clone()),
                        VisibleItem::GroupHeader { session_name, .. } => Some(session_name.clone()),
                    };
                    if let Some(target) = target {
                        if state.config.exit_on_switch {
                            state.should_quit = true;
                        }
                        return Some(Action::SwitchToPane(target));
                    }
                }
            }
            None
        }
        KeyCode::Char('r') => {
            if matches!(state.focus, Focus::Sessions) {
                if let Some(VisibleItem::Session { session, .. }) = state.visible_items.get(state.selected_index).cloned().as_ref() {
                    let pane_id = session.pane_id.clone();
                    state.unread_pane_ids.remove(&pane_id);
                    state::save_state(&state.unread_pane_ids, &state.prev_status_map);
                    refresh_visible_items(state);
                }
            }
            None
        }
        KeyCode::Char('c') => {
            if matches!(state.focus, Focus::Sessions) {
                if let Some(item) = state.visible_items.get(state.selected_index).cloned() {
                    let (session_name, cwd_target) = match &item {
                        VisibleItem::Session { session, .. } => (session.session_name.clone(), session.pane_target.clone()),
                        VisibleItem::GroupHeader { session_name, .. } => (session_name.clone(), session_name.clone()),
                    };
                    return Some(Action::CreateSession { session_name, cwd_target });
                }
            }
            None
        }
        KeyCode::Char('x') => {
            if matches!(state.focus, Focus::Sessions) {
                if let Some(VisibleItem::Session { session, .. }) = state.visible_items.get(state.selected_index).cloned().as_ref() {
                    state.pending_confirm_target = Some(session.pane_target.clone());
                }
            }
            None
        }
        KeyCode::Char('?') => {
            state.show_help = !state.show_help;
            None
        }
        KeyCode::Char('y') => {
            if matches!(state.focus, Focus::Preview) && !state.preview_content.is_empty() {
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    let _ = clipboard.set_text(&state.preview_content);
                    state.toast_message = Some("Copied!".to_string());
                    state.toast_deadline = Some(std::time::Instant::now() + std::time::Duration::from_millis(1500));
                }
            }
            None
        }
        _ => None,
    }
}

fn refresh_visible_items(state: &mut AppState) {
    let groups = group_sessions_by_name(&state.sessions);
    state.visible_items = build_visible_items(
        &groups,
        &state.collapsed_groups,
        &state.unread_pane_ids,
        &state.display_name_map,
    );
}

fn get_selected_pane_target(state: &AppState) -> Option<String> {
    state
        .visible_items
        .get(state.selected_index)
        .and_then(|item| match item {
            VisibleItem::Session { session, .. } => Some(session.pane_target.clone()),
            _ => None,
        })
}

fn update_selected_target(state: &AppState, selected_pane_target: &Arc<Mutex<Option<String>>>) {
    let target = get_selected_pane_target(state);
    if let Ok(mut lock) = selected_pane_target.try_lock() {
        *lock = target;
    }
}
