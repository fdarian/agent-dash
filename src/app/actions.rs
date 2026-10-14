use super::{mutate_store, now_ms, refresh_visible_items, show_toast, Action, AppState};
use crate::multiplexer::naming::{base_name, random_suffix, resolve_conflict};
use crate::multiplexer::tmux::TmuxClient;
use crate::multiplexer::{Multiplexer, PaneRef};
use crate::sessions::live::Mutation;
use crate::sessions::{basename, resume_command};

pub(super) async fn process_action(state: &mut AppState, action: Action) {
    let tmux = TmuxClient;
    match action {
        Action::SwitchToPane(pane) => {
            state.resize_paused = true;
            focus_or_toast(&tmux, state, &pane).await;
        }
        Action::OpenPopup(target) => {
            let _ = tmux.open_popup(&target).await;
        }
        Action::ResumeSession {
            session_id,
            label,
            cwd,
        } => {
            let command = match resume_command(&state.config.command, &session_id) {
                Ok(command) => command,
                Err(err) => {
                    show_toast(state, format!("{err:#}"), 4);
                    return;
                }
            };
            if let Some(pane) = spawn_session(&tmux, state, &label, &cwd, &command).await {
                mutate_store(
                    state,
                    vec![Mutation::SetUnread {
                        session_id,
                        unread: false,
                    }],
                );
                refresh_visible_items(state);
                switch_to_spawned(&tmux, state, &pane).await;
            }
        }
        Action::CreateSession { cwd } => {
            let command = state.config.command.clone();
            if let Some(pane) = spawn_session(&tmux, state, &basename(&cwd), &cwd, &command).await {
                state.book.expect_created(pane.pane_id.clone(), now_ms());
                switch_to_spawned(&tmux, state, &pane).await;
            }
        }
        Action::KillPane(pane) => {
            let _ = tmux.kill(&pane).await;
            // The session is archived by the next tick's tracking, not here.
        }
        Action::ForwardScrollDown { target, col, row } => {
            tokio::spawn(async move {
                let _ = crate::multiplexer::tmux::send_scroll_down(&target, col, row).await;
            });
        }
        Action::ForwardScrollUp { target, col, row } => {
            tokio::spawn(async move {
                let _ = crate::multiplexer::tmux::send_scroll_up(&target, col, row).await;
            });
        }
    }
}

/// Spawns a detached tmux session named after `label` running `command`. Failures are shown as
/// a toast and yield `None`.
async fn spawn_session<M: Multiplexer>(
    multiplexer: &M,
    state: &mut AppState,
    label: &str,
    cwd: &str,
    command: &str,
) -> Option<PaneRef> {
    if !std::path::Path::new(cwd).is_dir() {
        show_toast(state, format!("Directory no longer exists: {cwd}"), 4);
        return None;
    }
    let spawned = async {
        let name = resolve_conflict(
            &base_name(label),
            async |name: &str| multiplexer.name_taken(name).await,
            random_suffix,
        )
        .await?;
        multiplexer.spawn(&name, cwd, command).await
    }
    .await;
    match spawned {
        Ok(pane) => Some(pane),
        Err(err) => {
            show_toast(state, format!("Failed to start session: {err:#}"), 4);
            None
        }
    }
}

/// Same as `SwitchToPane` followed by the `exit_on_switch` handling in `open_selected`. Stays
/// open when focusing fails, so the toast can be read.
async fn switch_to_spawned<M: Multiplexer>(multiplexer: &M, state: &mut AppState, pane: &PaneRef) {
    state.resize_paused = true;
    if focus_or_toast(multiplexer, state, pane).await && state.config.exit_on_switch {
        request_exit(state);
    }
}

/// Returns whether the pane was focused.
async fn focus_or_toast<M: Multiplexer>(
    multiplexer: &M,
    state: &mut AppState,
    pane: &PaneRef,
) -> bool {
    match multiplexer.focus(pane).await {
        Ok(()) => true,
        Err(err) => {
            show_toast(state, format!("Failed to focus pane: {err:#}"), 4);
            false
        }
    }
}

/// Triggers the exit action. If `map_exit` is configured, stashes the command
/// in `pending_exit_cmd` so it can be spawned after the current action completes
/// (ensuring any tmux switch runs first). Otherwise sets `should_quit` directly.
pub(super) fn request_exit(state: &mut AppState) {
    match state.map_exit.clone() {
        Some(cmd) => state.pending_exit_cmd = Some(cmd),
        None => state.should_quit = true,
    }
}
