use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::watch;

use crate::app::actions::request_exit;
use crate::app::undo::UndoEntry;
use crate::app::view_config::ViewConfigOverlay;
use crate::app::{
    forward_scroll_target, get_selected_pane_target, mutate_store, refresh_keeping_selection,
    refresh_visible_items, save_ui_state, scroll_preview_down, scroll_preview_up, selected_session,
    show_toast, update_selected_target, Action, AppState, Focus, PreviewTarget,
};
use crate::copy_mode;
use crate::sessions::live::Mutation;
use crate::sessions::record::ArchiveReason;
use crate::sessions::view::VisibleItem;
use crate::ui::list_nav::{nav_action, NavAction, NavMode};

pub(super) fn handle(
    state: &mut AppState,
    key: KeyEvent,
    selected_pane_target: &watch::Sender<Option<PreviewTarget>>,
) -> Option<Action> {
    if let Some(nav) = nav_action(key, NavMode::List) {
        return handle_nav(state, nav, selected_pane_target);
    }
    match key.code {
        KeyCode::Char('q') => {
            request_exit(state);
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
        KeyCode::Char('h') => {
            if matches!(state.focus, Focus::Sessions) {
                if let Some(VisibleItem::Group(group)) =
                    state.visible_items.get(state.selected_index)
                {
                    let key = group.key.clone();
                    if !state.ui.collapsed_groups.remove(&key) {
                        state.ui.collapsed_groups.insert(key);
                    }
                    refresh_after_collapse_change(state, selected_pane_target);
                }
            }
            None
        }
        KeyCode::Char('l') => {
            if matches!(state.focus, Focus::Sessions) {
                if let Some(VisibleItem::Group(group)) =
                    state.visible_items.get(state.selected_index)
                {
                    if state.ui.collapsed_groups.remove(&group.key.clone()) {
                        refresh_after_collapse_change(state, selected_pane_target);
                    }
                }
            }
            None
        }
        KeyCode::Char('O') => get_selected_pane_target(state).map(Action::OpenPopup),
        KeyCode::Char('o') => open_selected(state),
        KeyCode::Char('r') => {
            if matches!(state.focus, Focus::Sessions) {
                if let Some(session) = selected_session(state) {
                    let mutation = Mutation::SetUnread {
                        session_id: session.record.session_id.clone(),
                        unread: !session.record.unread,
                    };
                    mutate_store(state, vec![mutation]);
                    refresh_visible_items(state);
                }
            }
            None
        }
        KeyCode::Char('c') => {
            if matches!(state.focus, Focus::Sessions) {
                let Some(session) = selected_session(state) else {
                    show_toast(state, "Select a session to create from", 2);
                    return None;
                };
                return Some(Action::CreateSession {
                    cwd: session.record.cwd.clone(),
                });
            }
            None
        }
        KeyCode::Char('a') => {
            if matches!(state.focus, Focus::Sessions) {
                if let Some(session) = selected_session(state) {
                    let session_id = session.record.session_id.clone();
                    let snapshot = session.record.archive_snapshot();
                    let mutation = if session.record.archived_at.is_some() {
                        Mutation::Unarchive {
                            session_id: session_id.clone(),
                        }
                    } else {
                        Mutation::Archive {
                            session_id: session_id.clone(),
                            reason: ArchiveReason::User,
                        }
                    };
                    mutate_store(state, vec![mutation]);
                    state.undo.push(UndoEntry::ArchiveToggle {
                        session_id,
                        snapshot,
                    });
                    refresh_keeping_selection(state);
                    update_selected_target(state, selected_pane_target);
                }
            }
            None
        }
        KeyCode::Char('u') => {
            if matches!(state.focus, Focus::Sessions) {
                undo_last(state, selected_pane_target);
            }
            None
        }
        KeyCode::Char('x') => {
            if matches!(state.focus, Focus::Sessions) {
                if let Some(pane) = selected_session(state).and_then(|s| s.pane()) {
                    state.pending_confirm_target = Some(pane.clone());
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
            } else if matches!(state.focus, Focus::Preview) {
                state.focus = Focus::Sessions;
            }
            None
        }
        KeyCode::Char('S') => {
            state.view_config = Some(ViewConfigOverlay::open());
            None
        }
        KeyCode::Char('`') => {
            let prior = state.ui.view;
            state.ui.view.grouping = state.ui.view.grouping.next();
            state.undo.push_view_change(prior, state.ui.view);
            save_ui_state(state);
            refresh_keeping_selection(state);
            update_selected_target(state, selected_pane_target);
            None
        }
        KeyCode::Char('n') => {
            if matches!(state.focus, Focus::Sessions) {
                if let Some(session) = selected_session(state) {
                    state.pending_note_edit = Some(crate::notes::session_note_path(session));
                }
            }
            None
        }
        _ => None,
    }
}

fn handle_nav(
    state: &mut AppState,
    nav: NavAction,
    selected_pane_target: &watch::Sender<Option<PreviewTarget>>,
) -> Option<Action> {
    match nav {
        NavAction::Next => move_cursor(state, true, selected_pane_target),
        NavAction::Prev => move_cursor(state, false, selected_pane_target),
        NavAction::Select => {
            if matches!(state.focus, Focus::Sessions) {
                return open_selected(state);
            }
            None
        }
        NavAction::Close => {
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
    }
}

pub(super) fn move_cursor(
    state: &mut AppState,
    down: bool,
    selected_pane_target: &watch::Sender<Option<PreviewTarget>>,
) -> Option<Action> {
    match state.focus {
        Focus::Sessions => {
            let last = state.visible_items.len().saturating_sub(1);
            let next = if down {
                (state.selected_index + 1).min(last)
            } else {
                state.selected_index.saturating_sub(1)
            };
            if next != state.selected_index {
                state.selected_index = next;
                state.preview_content.clear();
                state.preview_selection = None;
                update_selected_target(state, selected_pane_target);
            }
        }
        Focus::Preview => {
            let col = state.preview_pane_area.width / 2;
            let row = state.preview_pane_area.height / 2;
            if let Some((target, col, row)) = forward_scroll_target(state, col, row) {
                return Some(if down {
                    Action::ForwardScrollDown { target, col, row }
                } else {
                    Action::ForwardScrollUp { target, col, row }
                });
            }
            if down {
                scroll_preview_down(state);
            } else {
                scroll_preview_up(state);
            }
        }
    }
    None
}

fn refresh_after_collapse_change(
    state: &mut AppState,
    selected_pane_target: &watch::Sender<Option<PreviewTarget>>,
) {
    refresh_keeping_selection(state);
    update_selected_target(state, selected_pane_target);
    save_ui_state(state);
}

fn undo_last(state: &mut AppState, selected_pane_target: &watch::Sender<Option<PreviewTarget>>) {
    let Some(entry) = state.undo.pop() else {
        show_toast(state, "Nothing to undo", 2);
        return;
    };
    match &entry {
        UndoEntry::ArchiveToggle {
            session_id,
            snapshot,
        } => {
            if state.book.record(session_id).is_none() {
                show_toast(state, "Can't undo: the session no longer exists", 2);
                return;
            }
            // Before the store write so its error toast, if any, isn't overwritten.
            show_toast(state, entry.undone_message(), 2);
            mutate_store(
                state,
                vec![Mutation::RestoreArchive {
                    session_id: session_id.clone(),
                    snapshot: *snapshot,
                }],
            );
            refresh_keeping_selection(state);
            let restored = state.visible_items.iter().position(|item| {
                item.session()
                    .is_some_and(|session| session.record.session_id == *session_id)
            });
            if let Some(index) = restored {
                state.selected_index = index;
            }
        }
        UndoEntry::ViewChange { prior } => {
            show_toast(state, entry.undone_message(), 2);
            state.ui.view = *prior;
            save_ui_state(state);
            refresh_keeping_selection(state);
        }
    }
    update_selected_target(state, selected_pane_target);
}

/// `o` and Enter-in-filter: switch to the selected session's pane and clear its unread mark, or
/// resume the session in a new tmux session when it isn't running.
pub(super) fn open_selected(state: &mut AppState) -> Option<Action> {
    let session = selected_session(state)?;
    let Some(pane) = session.pane().cloned() else {
        return Some(Action::ResumeSession {
            session_id: session.record.session_id.clone(),
            label: session.display_title(),
            cwd: session.record.cwd.clone(),
        });
    };
    let session_id = session.record.session_id.clone();
    let was_unread = session.record.unread;
    if was_unread {
        mutate_store(
            state,
            vec![Mutation::SetUnread {
                session_id,
                unread: false,
            }],
        );
        refresh_visible_items(state);
    }
    if state.config.exit_on_switch {
        request_exit(state);
    }
    Some(Action::SwitchToPane(pane))
}
