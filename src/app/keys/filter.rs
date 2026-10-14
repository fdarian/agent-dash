use crossterm::event::KeyEvent;
use tokio::sync::watch;

use super::apply_text_input;
use super::sessions::{move_cursor, open_selected};
use crate::app::{
    refilter_visible_items, refresh_visible_items, update_selected_target, Action, AppState,
    PreviewTarget,
};
use crate::ui::list_nav::{nav_action, NavAction, NavMode};

pub(super) fn handle(
    state: &mut AppState,
    key: KeyEvent,
    selected_pane_target: &watch::Sender<Option<PreviewTarget>>,
) -> Option<Action> {
    match nav_action(key, NavMode::TextInput) {
        Some(NavAction::Close) => {
            state.session_filter_active = false;
            state.session_filter_query.clear();
            state.session_filter_cursor = 0;
            refresh_visible_items(state);
            state.selected_index = state
                .selected_index
                .min(state.visible_items.len().saturating_sub(1));
            update_selected_target(state, selected_pane_target);
            None
        }
        Some(NavAction::Select) => {
            state.session_filter_active = false;
            open_selected(state)
        }
        Some(NavAction::Next) => move_cursor(state, true, selected_pane_target),
        Some(NavAction::Prev) => move_cursor(state, false, selected_pane_target),
        _ => {
            let changed = apply_text_input(
                &mut state.session_filter_query,
                &mut state.session_filter_cursor,
                key,
            );
            if changed {
                refilter_visible_items(state);
                state.selected_index = 0;
                update_selected_target(state, selected_pane_target);
            }
            None
        }
    }
}
