use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers};
use futures::StreamExt;
use ratatui::prelude::*;
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;

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
}

pub enum Message {
    SessionsUpdated(Vec<ClaudeSession>, HashMap<String, String>),
}

pub async fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    exit_on_switch: bool,
) -> Result<()> {
    let config = crate::config::load_config(exit_on_switch);
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
    };

    // Load cached sessions for instant first render
    if let Some(cached) = load_cached_sessions() {
        state.sessions = cached.sessions;
        state.display_name_map = cached.display_names;
        refresh_visible_items(&mut state);
    }

    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    // Session polling task (every 2s)
    let poll_tx = tx.clone();
    tokio::spawn(async move {
        let config = crate::config::load_config(false);
        let tmux = TmuxClient::new(&config);
        loop {
            if let Ok(sessions) = tmux.discover_sessions().await {
                // For now, display names = session names (formatter comes in Phase 10)
                let mut display_names = HashMap::new();
                for s in &sessions {
                    display_names
                        .entry(s.session_name.clone())
                        .or_insert_with(|| s.session_name.clone());
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

    let mut event_stream = EventStream::new();

    // Initial render
    terminal.draw(|frame| ui::render(frame, &state))?;

    loop {
        tokio::select! {
            Some(Ok(event)) = event_stream.next() => {
                if let Event::Key(key) = event {
                    handle_key_event(&mut state, key);
                }
            }
            Some(msg) = rx.recv() => {
                handle_message(&mut state, msg);
            }
        }

        terminal.draw(|frame| ui::render(frame, &state))?;

        if state.should_quit {
            break;
        }
    }

    Ok(())
}

fn handle_message(state: &mut AppState, msg: Message) {
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
        }
    }
}

fn handle_key_event(state: &mut AppState, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => state.should_quit = true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.should_quit = true;
        }
        _ => {}
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
