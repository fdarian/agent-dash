use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::watch;

use super::apply_text_input;
use crate::app::view_config::Outcome;
use crate::app::{
    refresh_keeping_selection, save_ui_state, update_selected_target, Action, AppState,
    PreviewTarget,
};
use crate::ui::list_nav::{nav_action, NavAction, NavMode};

pub(super) fn handle_confirm(state: &mut AppState, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Enter => {
            let target = state.pending_confirm_target.take().unwrap();
            Some(Action::KillPane(target))
        }
        KeyCode::Esc => {
            state.pending_confirm_target = None;
            None
        }
        _ => None,
    }
}

pub(super) fn handle_help(state: &mut AppState, key: KeyEvent) -> Option<Action> {
    if state.help_filter_active {
        if nav_action(key, NavMode::TextInput) == Some(NavAction::Close) {
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
        None
    } else {
        if key.code == KeyCode::Char('?')
            || nav_action(key, NavMode::List) == Some(NavAction::Close)
        {
            state.show_help = false;
            state.help_filter_active = false;
            state.help_filter_query.clear();
            state.help_filter_cursor = 0;
        } else if key.code == KeyCode::Char('/') {
            state.help_filter_active = true;
        }
        None
    }
}

pub(super) fn handle_view_config(
    state: &mut AppState,
    key: KeyEvent,
    selected_pane_target: &watch::Sender<Option<PreviewTarget>>,
) -> Option<Action> {
    let overlay = state.view_config.as_mut()?;
    match overlay.handle_key(key, &mut state.ui.view) {
        Outcome::Unchanged => {}
        Outcome::Closed => state.view_config = None,
        Outcome::Changed => {
            save_ui_state(state);
            refresh_keeping_selection(state);
            update_selected_target(state, selected_pane_target);
        }
    }
    None
}
