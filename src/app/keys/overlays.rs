use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::watch;

use super::apply_text_input;
use crate::app::new_session;
use crate::app::view_config::Outcome;
use crate::app::{
    refresh_keeping_selection, save_ui_state, update_selected_target, Action, AppState,
    PreviewTarget,
};
use crate::ui::list_nav::{nav_action, NavAction, NavMode};
use crate::worktree::{load_repo_info, random_name, WorktreeRequest};

pub(super) fn handle_confirm(state: &mut AppState, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Enter => {
            let target = state.pending_confirm_target.take().unwrap();
            Some(Action::CloseSession(target))
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

pub(super) fn handle_new_session(state: &mut AppState, key: KeyEvent) -> Option<Action> {
    let dialog = state.new_session.as_mut()?;
    match dialog.handle_key(key, &load_repo_info) {
        new_session::Outcome::Pending => None,
        new_session::Outcome::Closed => {
            state.new_session = None;
            None
        }
        new_session::Outcome::Submit(submission) => {
            let worktree_offered = dialog.worktree_available();
            state.new_session = None;
            let checked = submission.worktree.is_some();
            if worktree_offered && state.ui.new_session_worktree != checked {
                state.ui.new_session_worktree = checked;
                save_ui_state(state);
            }
            Some(Action::CreateSession {
                cwd: submission.cwd,
                command: submission.command,
                worktree: submission.worktree.map(|request| match request {
                    WorktreeRequest::New { name, base } if name.is_empty() => {
                        WorktreeRequest::New {
                            name: random_name(),
                            base,
                        }
                    }
                    other => other,
                }),
            })
        }
    }
}

pub(super) fn handle_view_config(
    state: &mut AppState,
    key: KeyEvent,
    selected_pane_target: &watch::Sender<Option<PreviewTarget>>,
) -> Option<Action> {
    let prior = state.ui.view;
    let overlay = state.view_config.as_mut()?;
    match overlay.handle_key(key, &mut state.ui.view) {
        Outcome::Unchanged => {}
        Outcome::Closed => state.view_config = None,
        Outcome::Changed => {
            state.undo.push_view_change(prior, state.ui.view);
            save_ui_state(state);
            refresh_keeping_selection(state);
            update_selected_target(state, selected_pane_target);
        }
    }
    None
}
