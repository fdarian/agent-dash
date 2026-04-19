use anyhow::Result;
use crossterm::event::{
    Event, EventStream, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use futures::StreamExt;
use ratatui::prelude::*;
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;
use tokio::sync::watch;

use crate::cache::{load_cached_sessions, save_cached_sessions, CachedSessionData};
use crate::config::AppConfig;
use crate::copy_mode;
use crate::selection::{self, ContentPosition, PreviewSelection};
use crate::session::{
    auto_select_index, build_flat_visible_items, build_visible_items, group_sessions_by_name,
    resolve_selected_index, AgentSession, PromptState, SessionStatus, VisibleItem,
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
    pub sessions: Vec<AgentSession>,
    pub visible_items: Vec<VisibleItem>,
    pub selected_index: usize,
    pub focus: Focus,
    pub collapsed_groups: HashSet<String>,
    pub unread_pane_ids: HashSet<String>,
    pub prev_status_map: HashMap<String, SessionStatus>,
    pub display_name_map: HashMap<String, String>,
    pub prompt_states: HashMap<String, PromptState>,
    pub preview_content: String,
    pub preview_scroll_offset: u16,
    pub preview_is_sticky_bottom: bool,
    pub preview_content_height: u16,
    pub preview_area_height: u16,
    pub preview_pane_area: Rect,
    pub preview_selection: Option<PreviewSelection>,
    pub copy_mode: Option<copy_mode::CopyModeState>,
    pub pending_confirm_target: Option<String>,
    pub show_help: bool,
    pub sessions_expanded: bool,

    pub help_filter_active: bool,
    pub help_filter_query: String,
    pub help_filter_cursor: usize,

    pub session_filter_active: bool,
    pub session_filter_query: String,
    pub session_filter_cursor: usize,
    pub toast_message: Option<String>,
    pub toast_deadline: Option<std::time::Instant>,
    pub initial_focused_info: Option<(String, String)>,
    pub flat_view: bool,
    pub unread_order: HashMap<String, u64>,
    pub unread_counter: u64,
    pub hidden_pane_ids: HashSet<String>,
    pub hidden_groups: HashSet<String>,
    pub hidden_section_collapsed: bool,
}

pub enum Message {
    SessionsUpdated(
        Vec<AgentSession>,
        HashMap<String, String>,
        HashMap<String, PromptState>,
    ),
    PreviewUpdated(String),
}

pub enum Action {
    SwitchToPane(String),
    OpenPopup(String),
    CreateSession {
        tmux_session_name: String,
        cwd_target: String,
    },
    KillPane(String),
}

pub async fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    exit_on_switch: bool,
    exit_immediately: bool,
) -> Result<()> {
    let config = crate::config::load_config(exit_on_switch);
    let formatter_path = config.session_name_formatter.clone();
    let loaded_state = state::load_state();

    let focused_pane_info = {
        let tmux = TmuxClient::new(&config);
        tmux.get_focused_pane_info().await
    };

    let default_flat_view = config.default_flat_view;
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
        prompt_states: HashMap::new(),
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
        sessions_expanded: false,

        help_filter_active: false,
        help_filter_query: String::new(),
        help_filter_cursor: 0,

        session_filter_active: false,
        session_filter_query: String::new(),
        session_filter_cursor: 0,
        toast_message: None,
        toast_deadline: None,
        initial_focused_info: focused_pane_info,
        flat_view: default_flat_view,
        unread_order: loaded_state.unread_order,
        unread_counter: loaded_state.unread_counter,
        hidden_pane_ids: loaded_state.hidden_pane_ids,
        hidden_groups: loaded_state.hidden_groups,
        hidden_section_collapsed: true,
    };

    // Load cached sessions for instant first render
    if let Some(cached) = load_cached_sessions() {
        state.sessions = cached.sessions;
        state.display_name_map = cached.display_names;
        refresh_visible_items(&mut state);
        if let Some(info) = state.initial_focused_info.take() {
            state.selected_index = auto_select_index(&state.visible_items, &info.0, &info.1);
        }
    }

    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    let (target_tx, target_rx) = watch::channel(Option::<String>::None);

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
                    .map(|s| s.tmux_session_name.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();

                let mut display_names = HashMap::new();
                for name in &unique_names {
                    let formatted = if let Some(ref path) = formatter_path {
                        if let Some(cached) = formatter_cache.get(name) {
                            cached.clone()
                        } else {
                            match tokio::process::Command::new(path).arg(name).output().await {
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

                // Detect prompt states for idle sessions
                let mut prompt_set = tokio::task::JoinSet::new();
                for session in sessions.iter().filter(|s| s.status == SessionStatus::Idle) {
                    let target = session.pane_target.clone();
                    let pane_id = session.pane_id.clone();
                    prompt_set.spawn(async move {
                        let state = match crate::tmux::capture_pane_visible(&target).await {
                            Ok(text) => crate::session::detect_prompt_state(&text),
                            Err(_) => crate::session::PromptState::None,
                        };
                        (pane_id, state)
                    });
                }
                let mut prompt_states = HashMap::new();
                while let Some(result) = prompt_set.join_next().await {
                    if let Ok((pane_id, state)) = result {
                        prompt_states.insert(pane_id, state);
                    }
                }

                // Save to cache
                let cached_data = CachedSessionData {
                    sessions: sessions.clone(),
                    display_names: display_names.clone(),
                };
                save_cached_sessions(&cached_data);

                let _ = poll_tx.send(Message::SessionsUpdated(
                    sessions,
                    display_names,
                    prompt_states,
                ));
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    });

    // Preview task — pipe-pane notification with fallback polling
    let mut pipe_watcher = crate::pipe_pane::PipePaneWatcher::new();
    let fifo_path = pipe_watcher.fifo_path().to_string();
    crate::pipe_pane::spawn_preview_task(tx.clone(), target_rx, fifo_path);

    let mut event_stream = EventStream::new();

    // Initial render
    terminal.draw(|frame| ui::render(frame, &mut state))?;

    if exit_immediately {
        return Ok(());
    }

    loop {
        tokio::select! {
            Some(Ok(event)) = event_stream.next() => {
                match event {
                    Event::Key(key) => {
                        let action = handle_key_event(&mut state, key, &target_tx);
                        if let Some(action) = action {
                            process_action(&mut state, action, &target_tx).await;
                        }
                    }
                    Event::Mouse(mouse) => {
                        handle_mouse_event(&mut state, mouse);
                    }
                    _ => {}
                }
            }
            Some(msg) = rx.recv() => {
                handle_message(&mut state, msg, &target_tx);
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

    pipe_watcher.cleanup();

    Ok(())
}

async fn process_action(
    state: &mut AppState,
    action: Action,
    selected_pane_target: &watch::Sender<Option<String>>,
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
        Action::CreateSession {
            tmux_session_name,
            cwd_target,
        } => {
            let config = crate::config::load_config(state.config.exit_on_switch);
            let tmux = TmuxClient::new(&config);
            if let Ok(cwd) = tmux.get_pane_cwd(&cwd_target).await {
                if let Ok(Some(pane_info)) =
                    tmux.create_window(&tmux_session_name, Some(&cwd)).await
                {
                    let _ = tmux.switch_to_pane(&pane_info.pane_target).await;
                    if state.config.exit_on_switch {
                        state.should_quit = true;
                    } else {
                        let new_session = AgentSession {
                            pane_id: pane_info.pane_id,
                            pane_target: pane_info.pane_target,
                            title: pane_info.pane_title.clone(),
                            tmux_session_name: pane_info.tmux_session_name.clone(),
                            status: crate::session::parse_session_status(&pane_info.pane_title),
                        };
                        state
                            .prev_status_map
                            .insert(new_session.pane_id.clone(), new_session.status.clone());
                        state.sessions.push(new_session);
                        persist_state(state);
                        let old_items = std::mem::take(&mut state.visible_items);
                        refresh_visible_items(state);
                        state.selected_index = resolve_selected_index(
                            &state.visible_items,
                            &old_items,
                            state.selected_index,
                        );
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
                state.unread_order.remove(&pane_id);
            }
            state.sessions.retain(|s| s.pane_target != target);
            persist_state(state);
            let old_items = std::mem::take(&mut state.visible_items);
            refresh_visible_items(state);
            state.selected_index =
                resolve_selected_index(&state.visible_items, &old_items, state.selected_index);
            update_selected_target(state, selected_pane_target);
        }
    }
}

fn handle_message(
    state: &mut AppState,
    msg: Message,
    selected_pane_target: &watch::Sender<Option<String>>,
) {
    match msg {
        Message::SessionsUpdated(sessions, display_names, prompt_states) => {
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
                        state.unread_counter += 1;
                        state
                            .unread_order
                            .insert(session.pane_id.clone(), state.unread_counter);
                    }
                }
            }

            // Remove unread for panes that no longer exist
            next_unread.retain(|id| current_pane_ids.contains(id));
            state
                .unread_order
                .retain(|id, _| current_pane_ids.contains(id));

            // Update prev status map
            let mut next_status_map = HashMap::new();
            for session in &sessions {
                next_status_map.insert(session.pane_id.clone(), session.status.clone());
            }

            state.sessions = sessions;
            state.display_name_map = display_names;
            state.prompt_states = prompt_states;
            state.prev_status_map = next_status_map;
            state.unread_pane_ids = next_unread;

            // Persist state
            persist_state(state);

            // Resolve selected index
            let old_items = std::mem::take(&mut state.visible_items);
            refresh_visible_items(state);
            if let Some(info) = state.initial_focused_info.take() {
                state.selected_index = auto_select_index(&state.visible_items, &info.0, &info.1);
            } else {
                state.selected_index =
                    resolve_selected_index(&state.visible_items, &old_items, state.selected_index);
            }

            update_selected_target(state, selected_pane_target);
        }
        Message::PreviewUpdated(content) => {
            if state.copy_mode.is_some() {
                return;
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
    }
}

fn apply_text_input(query: &mut String, cursor: &mut usize, key: KeyEvent) -> bool {
    match (key.code, key.modifiers) {
        (KeyCode::Char('a'), KeyModifiers::CONTROL) => {
            *cursor = 0;
            false
        }
        (KeyCode::Char('e'), KeyModifiers::CONTROL) => {
            *cursor = query.chars().count();
            false
        }
        (KeyCode::Char('u'), KeyModifiers::CONTROL) | (KeyCode::Backspace, KeyModifiers::SUPER) => {
            let byte_offset = query
                .char_indices()
                .nth(*cursor)
                .map(|(i, _)| i)
                .unwrap_or(query.len());
            query.drain(..byte_offset);
            *cursor = 0;
            true
        }
        (KeyCode::Char('k'), KeyModifiers::CONTROL) => {
            let byte_offset = query
                .char_indices()
                .nth(*cursor)
                .map(|(i, _)| i)
                .unwrap_or(query.len());
            query.truncate(byte_offset);
            true
        }
        (KeyCode::Char('b'), KeyModifiers::CONTROL) | (KeyCode::Left, KeyModifiers::NONE) => {
            if *cursor > 0 {
                *cursor -= 1;
            }
            false
        }
        (KeyCode::Char('f'), KeyModifiers::CONTROL) | (KeyCode::Right, KeyModifiers::NONE) => {
            let len = query.chars().count();
            if *cursor < len {
                *cursor += 1;
            }
            false
        }
        (KeyCode::Left, KeyModifiers::ALT) => {
            let chars: Vec<char> = query.chars().collect();
            let mut pos = *cursor;
            while pos > 0 && chars[pos - 1].is_whitespace() {
                pos -= 1;
            }
            while pos > 0 && !chars[pos - 1].is_whitespace() {
                pos -= 1;
            }
            *cursor = pos;
            false
        }
        (KeyCode::Right, KeyModifiers::ALT) => {
            let chars: Vec<char> = query.chars().collect();
            let len = chars.len();
            let mut pos = *cursor;
            while pos < len && !chars[pos].is_whitespace() {
                pos += 1;
            }
            while pos < len && chars[pos].is_whitespace() {
                pos += 1;
            }
            *cursor = pos;
            false
        }
        (KeyCode::Backspace, KeyModifiers::ALT) => {
            let chars: Vec<char> = query.chars().collect();
            let mut pos = *cursor;
            while pos > 0 && chars[pos - 1].is_whitespace() {
                pos -= 1;
            }
            while pos > 0 && !chars[pos - 1].is_whitespace() {
                pos -= 1;
            }
            let start_byte = query
                .char_indices()
                .nth(pos)
                .map(|(i, _)| i)
                .unwrap_or(query.len());
            let end_byte = query
                .char_indices()
                .nth(*cursor)
                .map(|(i, _)| i)
                .unwrap_or(query.len());
            query.drain(start_byte..end_byte);
            *cursor = pos;
            true
        }
        (KeyCode::Backspace, _) => {
            if *cursor > 0 {
                let byte_at_cursor = query
                    .char_indices()
                    .nth(*cursor - 1)
                    .map(|(i, _)| i)
                    .unwrap_or(query.len());
                let next_byte = query
                    .char_indices()
                    .nth(*cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(query.len());
                query.drain(byte_at_cursor..next_byte);
                *cursor -= 1;
                true
            } else {
                false
            }
        }
        (KeyCode::Delete, _) => {
            let len = query.chars().count();
            if *cursor < len {
                let byte_at_cursor = query
                    .char_indices()
                    .nth(*cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(query.len());
                let next_byte = query
                    .char_indices()
                    .nth(*cursor + 1)
                    .map(|(i, _)| i)
                    .unwrap_or(query.len());
                query.drain(byte_at_cursor..next_byte);
                true
            } else {
                false
            }
        }
        (KeyCode::Char(c), _) => {
            let byte_offset = query
                .char_indices()
                .nth(*cursor)
                .map(|(i, _)| i)
                .unwrap_or(query.len());
            query.insert(byte_offset, c);
            *cursor += 1;
            true
        }
        _ => false,
    }
}

fn handle_key_event(
    state: &mut AppState,
    key: KeyEvent,
    selected_pane_target: &watch::Sender<Option<String>>,
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
            if matches!(key.code, KeyCode::Esc) {
                state.help_filter_active = false;
                state.help_filter_query.clear();
                state.help_filter_cursor = 0;
                return None;
            }
            apply_text_input(
                &mut state.help_filter_query,
                &mut state.help_filter_cursor,
                key,
            );
            return None;
        } else {
            match key.code {
                KeyCode::Char('?') | KeyCode::Esc => {
                    state.show_help = false;
                    state.help_filter_active = false;
                    state.help_filter_query.clear();
                    state.help_filter_cursor = 0;
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

    // Session filter input mode
    if state.session_filter_active {
        match key.code {
            KeyCode::Esc => {
                state.session_filter_active = false;
                state.session_filter_query.clear();
                state.session_filter_cursor = 0;
                refresh_visible_items(state);
                state.selected_index = state
                    .selected_index
                    .min(state.visible_items.len().saturating_sub(1));
                update_selected_target(state, selected_pane_target);
                return None;
            }
            KeyCode::Enter => {
                state.session_filter_active = false;
                if let Some(item) = state.visible_items.get(state.selected_index).cloned() {
                    if let VisibleItem::Session { ref session, .. } = item {
                        state.unread_pane_ids.remove(&session.pane_id);
                        state.unread_order.remove(&session.pane_id);
                        persist_state(state);
                        refresh_visible_items(state);
                    }
                    let target = match &item {
                        VisibleItem::Session { session, .. } => Some(session.pane_target.clone()),
                        VisibleItem::GroupHeader {
                            tmux_session_name, ..
                        } => Some(tmux_session_name.clone()),
                        VisibleItem::HiddenHeader { .. } => None,
                    };
                    if let Some(target) = target {
                        if state.config.exit_on_switch {
                            state.should_quit = true;
                        }
                        return Some(Action::SwitchToPane(target));
                    }
                }
                return None;
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let max = state.visible_items.len().saturating_sub(1);
                state.selected_index = (state.selected_index + 1).min(max);
                update_selected_target(state, selected_pane_target);
                return None;
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                state.selected_index = state.selected_index.saturating_sub(1);
                update_selected_target(state, selected_pane_target);
                return None;
            }
            _ => {
                let changed = apply_text_input(
                    &mut state.session_filter_query,
                    &mut state.session_filter_cursor,
                    key,
                );
                if changed {
                    refresh_visible_items(state);
                    state.selected_index = 0;
                    update_selected_target(state, selected_pane_target);
                }
                return None;
            }
        }
    }

    if state.copy_mode.is_some() {
        if state.copy_mode.as_ref().unwrap().search_active {
            return copy_mode::handle_copy_mode_search_input(state, key);
        } else {
            return copy_mode::handle_copy_mode_key(state, key);
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
            state.sessions_expanded = false;
            state.focus = Focus::Preview;
            None
        }
        KeyCode::Char('j') | KeyCode::Down => {
            match state.focus {
                Focus::Sessions => {
                    if state.selected_index < state.visible_items.len().saturating_sub(1) {
                        state.selected_index += 1;
                        state.preview_content.clear();
                        state.preview_selection = None;
                        update_selected_target(state, selected_pane_target);
                    }
                }
                Focus::Preview => {
                    scroll_preview_down(state);
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
                        state.preview_selection = None;
                        update_selected_target(state, selected_pane_target);
                    }
                }
                Focus::Preview => {
                    scroll_preview_up(state);
                }
            }
            None
        }
        KeyCode::Char('h') => {
            if matches!(state.focus, Focus::Sessions) {
                if let Some(item) = state.visible_items.get(state.selected_index).cloned() {
                    match &item {
                        VisibleItem::HiddenHeader { .. } => {
                            state.hidden_section_collapsed = !state.hidden_section_collapsed;
                            refresh_visible_items(state);
                        }
                        VisibleItem::GroupHeader {
                            tmux_session_name, ..
                        } => {
                            let group_name = tmux_session_name.clone();
                            // Clear individual pane hides for this group
                            state.hidden_pane_ids.retain(|pid| {
                                !state
                                    .sessions
                                    .iter()
                                    .any(|s| s.tmux_session_name == group_name && s.pane_id == *pid)
                            });
                            if !state.hidden_groups.remove(&group_name) {
                                state.hidden_groups.insert(group_name);
                            }
                            hide_toggle_refresh(state, selected_pane_target);
                        }
                        VisibleItem::Session { session, .. } => {
                            let pane_id = session.pane_id.clone();
                            if state.hidden_groups.contains(&session.tmux_session_name) {
                                // Unhide this session from a group-level hide:
                                // remove group hide, individually hide all siblings instead
                                let group_name = session.tmux_session_name.clone();
                                state.hidden_groups.remove(&group_name);
                                for s in &state.sessions {
                                    if s.tmux_session_name == group_name && s.pane_id != pane_id {
                                        state.hidden_pane_ids.insert(s.pane_id.clone());
                                    }
                                }
                            } else if !state.hidden_pane_ids.remove(&pane_id) {
                                state.hidden_pane_ids.insert(pane_id);
                            }
                            hide_toggle_refresh(state, selected_pane_target);
                        }
                    }
                }
            }
            None
        }
        KeyCode::Char('l') => {
            if matches!(state.focus, Focus::Sessions) {
                if let Some(item) = state.visible_items.get(state.selected_index).cloned() {
                    match &item {
                        VisibleItem::GroupHeader {
                            tmux_session_name, ..
                        } => {
                            let name = tmux_session_name.clone();
                            state.collapsed_groups.remove(&name);
                            refresh_visible_items(state);
                            update_selected_target(state, selected_pane_target);
                        }
                        VisibleItem::HiddenHeader { is_collapsed, .. } if *is_collapsed => {
                            state.hidden_section_collapsed = false;
                            refresh_visible_items(state);
                        }
                        _ => {}
                    }
                }
            }
            None
        }
        KeyCode::Char('O') => get_selected_pane_target(state).map(Action::OpenPopup),
        KeyCode::Char('o') => {
            if let Some(item) = state.visible_items.get(state.selected_index).cloned() {
                if let VisibleItem::Session { ref session, .. } = item {
                    state.unread_pane_ids.remove(&session.pane_id);
                    state.unread_order.remove(&session.pane_id);
                    persist_state(state);
                    refresh_visible_items(state);
                }
                let target = match &item {
                    VisibleItem::Session { session, .. } => Some(session.pane_target.clone()),
                    VisibleItem::GroupHeader {
                        tmux_session_name, ..
                    } => Some(tmux_session_name.clone()),
                    VisibleItem::HiddenHeader { .. } => None,
                };
                if let Some(target) = target {
                    if state.config.exit_on_switch {
                        state.should_quit = true;
                    }
                    return Some(Action::SwitchToPane(target));
                }
            }
            None
        }
        KeyCode::Char('r') => {
            if matches!(state.focus, Focus::Sessions) {
                if let Some(VisibleItem::Session { session, .. }) = state
                    .visible_items
                    .get(state.selected_index)
                    .cloned()
                    .as_ref()
                {
                    let pane_id = session.pane_id.clone();
                    state.unread_pane_ids.remove(&pane_id);
                    state.unread_order.remove(&pane_id);
                    persist_state(state);
                    refresh_visible_items(state);
                }
            }
            None
        }
        KeyCode::Char('c') => {
            if matches!(state.focus, Focus::Sessions) {
                if let Some(item) = state.visible_items.get(state.selected_index).cloned() {
                    let (tmux_session_name, cwd_target) = match &item {
                        VisibleItem::Session { session, .. } => (
                            session.tmux_session_name.clone(),
                            session.pane_target.clone(),
                        ),
                        VisibleItem::GroupHeader {
                            tmux_session_name, ..
                        } => (tmux_session_name.clone(), tmux_session_name.clone()),
                        VisibleItem::HiddenHeader { .. } => return None,
                    };
                    return Some(Action::CreateSession {
                        tmux_session_name,
                        cwd_target,
                    });
                }
            }
            None
        }
        KeyCode::Char('x') => {
            if matches!(state.focus, Focus::Sessions) {
                if let Some(VisibleItem::Session { session, .. }) = state
                    .visible_items
                    .get(state.selected_index)
                    .cloned()
                    .as_ref()
                {
                    state.pending_confirm_target = Some(session.pane_target.clone());
                }
            }
            None
        }
        KeyCode::Char('/') => {
            if matches!(state.focus, Focus::Sessions) {
                state.session_filter_active = true;
                state.session_filter_query.clear();
                state.session_filter_cursor = 0;
            } else if matches!(state.focus, Focus::Preview) && !state.preview_content.is_empty() {
                state.preview_is_sticky_bottom = false;
                state.preview_selection = None;
                let visible_height = state.preview_area_height.saturating_sub(2);
                let middle_row = (state.preview_scroll_offset + visible_height / 2)
                    .min(state.preview_content_height.saturating_sub(1));
                let mut copy = copy_mode::CopyModeState::new(middle_row, 0);
                copy.search_active = true;
                copy.search_direction = copy_mode::SearchDirection::Forward;
                state.copy_mode = Some(copy);
            }
            None
        }
        KeyCode::Esc => {
            if matches!(state.focus, Focus::Sessions) && !state.session_filter_query.is_empty() {
                state.session_filter_query.clear();
                state.session_filter_cursor = 0;
                refresh_visible_items(state);
                state.selected_index = state
                    .selected_index
                    .min(state.visible_items.len().saturating_sub(1));
                update_selected_target(state, selected_pane_target);
            }
            None
        }
        KeyCode::Char('?') => {
            if matches!(state.focus, Focus::Preview) && !state.preview_content.is_empty() {
                state.preview_is_sticky_bottom = false;
                state.preview_selection = None;
                let visible_height = state.preview_area_height.saturating_sub(2);
                let middle_row = (state.preview_scroll_offset + visible_height / 2)
                    .min(state.preview_content_height.saturating_sub(1));
                let mut copy = copy_mode::CopyModeState::new(middle_row, 0);
                copy.search_active = true;
                copy.search_direction = copy_mode::SearchDirection::Backward;
                state.copy_mode = Some(copy);
            } else {
                state.show_help = !state.show_help;
            }
            None
        }
        KeyCode::Char('v') => {
            if !state.preview_content.is_empty() {
                state.focus = Focus::Preview;
                state.sessions_expanded = false;
                state.preview_is_sticky_bottom = false;
                state.preview_selection = None;
                let visible_height = state.preview_area_height.saturating_sub(2);
                let middle_row = (state.preview_scroll_offset + visible_height / 2)
                    .min(state.preview_content_height.saturating_sub(1));
                state.copy_mode = Some(copy_mode::CopyModeState::new(middle_row, 0));
            }
            None
        }
        KeyCode::Char('y') => {
            if matches!(state.focus, Focus::Preview) && !state.preview_content.is_empty() {
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    let _ = clipboard.set_text(&state.preview_content);
                    state.toast_message = Some("Copied!".to_string());
                    state.toast_deadline =
                        Some(std::time::Instant::now() + std::time::Duration::from_millis(1500));
                }
            }
            None
        }
        KeyCode::Char('+') => {
            if matches!(state.focus, Focus::Sessions) && !state.sessions_expanded {
                state.sessions_expanded = true;
            }
            None
        }
        KeyCode::Char('_') => {
            if matches!(state.focus, Focus::Sessions) && state.sessions_expanded {
                state.sessions_expanded = false;
            }
            None
        }
        KeyCode::Char('`') => {
            state.flat_view = !state.flat_view;
            let old_items = std::mem::take(&mut state.visible_items);
            refresh_visible_items(state);
            state.selected_index =
                resolve_selected_index(&state.visible_items, &old_items, state.selected_index);
            update_selected_target(state, selected_pane_target);
            None
        }
        _ => None,
    }
}

fn handle_mouse_event(state: &mut AppState, mouse: MouseEvent) {
    if state.pending_confirm_target.is_some() || state.show_help {
        return;
    }

    if state.copy_mode.is_some() && matches!(mouse.kind, MouseEventKind::Down(_)) {
        state.copy_mode = None;
        state.preview_selection = None;
    }

    let in_preview = mouse.column >= state.preview_pane_area.x
        && mouse.column < state.preview_pane_area.x + state.preview_pane_area.width
        && mouse.row >= state.preview_pane_area.y
        && mouse.row < state.preview_pane_area.y + state.preview_pane_area.height;

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if !in_preview {
                state.preview_selection = None;
                return;
            }
            if let Some(pos) = selection::mouse_to_content_position(
                mouse.column,
                mouse.row,
                state.preview_pane_area,
                state.preview_scroll_offset,
            ) {
                state.preview_selection = Some(PreviewSelection {
                    anchor: ContentPosition {
                        row: pos.row,
                        col: pos.col,
                    },
                    cursor: ContentPosition {
                        row: pos.row,
                        col: pos.col,
                    },
                    is_dragging: true,
                });
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(ref mut sel) = state.preview_selection {
                // Clamp mouse coords to inner area bounds
                let inner_x = state.preview_pane_area.x + 1;
                let inner_y = state.preview_pane_area.y + 1;
                let inner_right = state.preview_pane_area.x + state.preview_pane_area.width - 1;
                let inner_bottom = state.preview_pane_area.y + state.preview_pane_area.height - 1;

                let clamped_col = mouse.column.clamp(inner_x, inner_right.saturating_sub(1));
                let clamped_row = mouse.row.clamp(inner_y, inner_bottom.saturating_sub(1));

                sel.cursor.col = clamped_col - inner_x;
                sel.cursor.row = (clamped_row - inner_y) + state.preview_scroll_offset;
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if let Some(ref mut sel) = state.preview_selection {
                sel.is_dragging = false;
                if sel.anchor.row == sel.cursor.row && sel.anchor.col == sel.cursor.col {
                    state.preview_selection = None;
                } else if !state.preview_content.is_empty() {
                    let text = ansi_to_tui::IntoText::into_text(&state.preview_content)
                        .unwrap_or_default();
                    let selected = selection::extract_selected_text(&text, sel);
                    if !selected.is_empty() {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(&selected);
                            state.toast_message = Some("Copied to clipboard!".to_string());
                            state.toast_deadline = Some(
                                std::time::Instant::now() + std::time::Duration::from_millis(1500),
                            );
                        }
                    }
                }
            }
        }
        MouseEventKind::ScrollDown if in_preview => scroll_preview_down(state),
        MouseEventKind::ScrollUp if in_preview => scroll_preview_up(state),
        _ => {}
    }
}

fn refresh_visible_items(state: &mut AppState) {
    if state.flat_view {
        state.visible_items = build_flat_visible_items(
            &state.sessions,
            &state.unread_pane_ids,
            &state.unread_order,
            &state.prompt_states,
            &state.display_name_map,
            &state.hidden_pane_ids,
            &state.hidden_groups,
            state.hidden_section_collapsed,
        );
    } else {
        let groups = group_sessions_by_name(&state.sessions);
        state.visible_items = build_visible_items(
            &groups,
            &state.collapsed_groups,
            &state.unread_pane_ids,
            &state.unread_order,
            &state.prompt_states,
            &state.display_name_map,
            &state.hidden_pane_ids,
            &state.hidden_groups,
            state.hidden_section_collapsed,
        );
    }

    if !state.session_filter_query.is_empty() {
        use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
        use nucleo_matcher::{Matcher, Utf32Str};

        let mut matcher = Matcher::new(nucleo_matcher::Config::DEFAULT);
        let pattern = Pattern::parse(
            &state.session_filter_query,
            CaseMatching::Smart,
            Normalization::Smart,
        );

        let items = std::mem::take(&mut state.visible_items);
        let mut scored: Vec<(u32, VisibleItem)> = items
            .into_iter()
            .filter_map(|item| match &item {
                VisibleItem::Session {
                    session,
                    display_name,
                    ..
                } => {
                    let haystack = if session.title.is_empty() {
                        display_name.clone()
                    } else {
                        format!("{}/{}", display_name, session.title)
                    };
                    let mut buf = Vec::new();
                    let score = pattern.score(Utf32Str::new(&haystack, &mut buf), &mut matcher)?;
                    Some((score, item))
                }
                _ => None,
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        state.visible_items = scored.into_iter().map(|(_, item)| item).collect();
    }
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

fn update_selected_target(state: &AppState, selected_pane_target: &watch::Sender<Option<String>>) {
    let target = get_selected_pane_target(state);
    let _ = selected_pane_target.send(target);
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

fn persist_state(state: &AppState) {
    state::save_state(
        &state.unread_pane_ids,
        &state.prev_status_map,
        &state.unread_order,
        state.unread_counter,
        &state.hidden_pane_ids,
        &state.hidden_groups,
    );
}

fn hide_toggle_refresh(state: &mut AppState, selected_pane_target: &watch::Sender<Option<String>>) {
    persist_state(state);
    let old_items = std::mem::take(&mut state.visible_items);
    refresh_visible_items(state);
    state.selected_index =
        resolve_selected_index(&state.visible_items, &old_items, state.selected_index);
    update_selected_target(state, selected_pane_target);
}
