mod filter;
mod overlays;
mod sessions;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::watch;

use super::{Action, AppState, PreviewTarget};
use crate::copy_mode;

pub(super) fn handle_key_event(
    state: &mut AppState,
    key: KeyEvent,
    selected_pane_target: &watch::Sender<Option<PreviewTarget>>,
) -> Option<Action> {
    // Confirm dialog takes priority over all other input
    if state.pending_confirm_target.is_some() {
        return overlays::handle_confirm(state, key);
    }

    // Help overlay takes priority over main input
    if state.show_help {
        return overlays::handle_help(state, key);
    }

    if state.new_session.is_some() {
        return overlays::handle_new_session(state, key);
    }

    if state.view_config.is_some() {
        return overlays::handle_view_config(state, key, selected_pane_target);
    }

    if state.session_filter_active {
        return filter::handle(state, key, selected_pane_target);
    }

    if state.copy_mode.is_some() {
        if state.copy_mode.as_ref().unwrap().search_active {
            return copy_mode::handle_copy_mode_search_input(state, key);
        } else {
            return copy_mode::handle_copy_mode_key(state, key);
        }
    }

    sessions::handle(state, key, selected_pane_target)
}

pub(super) fn apply_text_input(query: &mut String, cursor: &mut usize, key: KeyEvent) -> bool {
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
        (KeyCode::Backspace, _) if *cursor > 0 => {
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
        }
        (KeyCode::Backspace, _) => false,
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
