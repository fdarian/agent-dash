use std::collections::{HashMap, HashSet};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::tmux::TmuxClient;

const MIN_COLS: u16 = 40;
const MIN_ROWS: u16 = 10;

pub struct ResizeRequest {
    pub pane_target: String,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Default)]
struct ResizeState {
    last_applied: Option<(String, u16, u16)>,
    configured_sessions: HashSet<String>,
    touched_windows: HashSet<String>,
}

fn parse_session_window(pane_target: &str) -> Option<(String, String)> {
    let dot_pos = pane_target.rfind('.')?;
    let session_window = &pane_target[..dot_pos];
    let session = session_window.split(':').next()?;
    if session.is_empty() || session_window.is_empty() {
        return None;
    }
    Some((session_window.to_string(), session.to_string()))
}

pub fn spawn_resize_task(mut request_rx: watch::Receiver<Option<ResizeRequest>>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let config = crate::config::load_config(false);
        let tmux = TmuxClient::new(&config);

        let mut state = ResizeState::default();

        let debounce_duration = tokio::time::Duration::from_millis(150);
        let mut debounce: Option<(tokio::time::Instant, String, u16, u16)> = None;

        loop {
            let debounce_sleep = match debounce {
                Some((deadline, _, _, _)) => tokio::time::sleep_until(deadline),
                None => tokio::time::sleep(tokio::time::Duration::from_secs(86400)),
            };

            tokio::select! {
                result = request_rx.changed() => {
                    if result.is_err() {
                        break;
                    }

                    let (pane_target, cols, rows) = {
                        let req = request_rx.borrow_and_update();
                        match req.as_ref() {
                            Some(r) => (r.pane_target.clone(), r.cols, r.rows),
                            None => {
                                debounce = None;
                                continue;
                            }
                        }
                    };

                    if cols < MIN_COLS || rows < MIN_ROWS {
                        debounce = None;
                        continue;
                    }

                    let (session_window, session) = match parse_session_window(&pane_target) {
                        Some(pair) => pair,
                        None => {
                            debounce = None;
                            continue;
                        }
                    };

                    let target_changed = state
                        .last_applied
                        .as_ref()
                        .map(|(t, _, _)| t != &session_window)
                        .unwrap_or(true);

                    if target_changed {
                        debounce = None;
                        apply_resize(&tmux, &session_window, &session, cols, rows, &mut state).await;
                    } else {
                        let same_dims = state
                            .last_applied
                            .as_ref()
                            .map(|(_, c, r)| *c == cols && *r == rows)
                            .unwrap_or(false);
                        if !same_dims {
                            debounce = Some((
                                tokio::time::Instant::now() + debounce_duration,
                                session_window,
                                cols,
                                rows,
                            ));
                        }
                    }
                }

                _ = debounce_sleep, if debounce.is_some() => {
                    if let Some((_, session_window, cols, rows)) = debounce.take() {
                        let session = match session_window.split(':').next() {
                            Some(s) if !s.is_empty() => s.to_string(),
                            _ => continue,
                        };
                        apply_resize(&tmux, &session_window, &session, cols, rows, &mut state).await;
                    }
                }
            }
        }

        restore_windows(&tmux, &state).await;
    })
}

async fn apply_resize(
    tmux: &TmuxClient<'_>,
    session_window: &str,
    session: &str,
    cols: u16,
    rows: u16,
    state: &mut ResizeState,
) {
    if !state.configured_sessions.contains(session) {
        let _ = tmux.set_window_size_manual(session).await;
        state.configured_sessions.insert(session.to_string());
    }
    let _ = tmux.resize_window(session_window, cols, rows).await;
    state.touched_windows.insert(session_window.to_string());
    state.last_applied = Some((session_window.to_string(), cols, rows));
}

async fn restore_windows(tmux: &TmuxClient<'_>, state: &ResizeState) {
    let mut by_session: HashMap<String, Vec<String>> = HashMap::new();
    for window in &state.touched_windows {
        let session = match window.split(':').next() {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        by_session.entry(session).or_default().push(window.clone());
    }

    for (session, windows) in by_session {
        if let Ok(Some((w, h))) = tmux.get_client_size(&session).await {
            for window in &windows {
                let _ = tmux.resize_window(window, w, h).await;
            }
        }
    }

    for session in &state.configured_sessions {
        let _ = tmux.unset_window_size(session).await;
    }
}
