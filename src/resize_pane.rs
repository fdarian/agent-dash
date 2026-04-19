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
    zoomed: Option<ZoomState>,
}

struct ZoomState {
    pane_target: String,
    we_zoomed_it: bool,
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
        let mut debounce: Option<(tokio::time::Instant, String, String, u16, u16)> = None;

        loop {
            let debounce_sleep = match debounce {
                Some((deadline, _, _, _, _)) => tokio::time::sleep_until(deadline),
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

                    let session_window = match parse_session_window(&pane_target) {
                        Some((sw, _)) => sw,
                        None => {
                            debounce = None;
                            continue;
                        }
                    };

                    let target_changed = state
                        .last_applied
                        .as_ref()
                        .map(|(p, _, _)| p != &pane_target)
                        .unwrap_or(true);

                    if target_changed {
                        debounce = None;
                        apply_resize(&tmux, &pane_target, &session_window, cols, rows, &mut state).await;
                    } else {
                        let same_dims = state
                            .last_applied
                            .as_ref()
                            .map(|(_, c, r)| *c == cols && *r == rows)
                            .unwrap_or(false);
                        if !same_dims {
                            debounce = Some((
                                tokio::time::Instant::now() + debounce_duration,
                                pane_target,
                                session_window,
                                cols,
                                rows,
                            ));
                        }
                    }
                }

                _ = debounce_sleep, if debounce.is_some() => {
                    if let Some((_, pane_target, session_window, cols, rows)) = debounce.take() {
                        apply_resize(&tmux, &pane_target, &session_window, cols, rows, &mut state).await;
                    }
                }
            }
        }

        restore_windows(&tmux, &state).await;
    })
}

async fn apply_resize(
    tmux: &TmuxClient<'_>,
    pane_target: &str,
    session_window: &str,
    cols: u16,
    rows: u16,
    state: &mut ResizeState,
) {
    let session = match session_window.split(':').next() {
        Some(s) if !s.is_empty() => s,
        _ => return,
    };

    transition_zoom(tmux, pane_target, &mut state.zoomed).await;

    if !state.configured_sessions.contains(session) {
        let _ = tmux.set_window_size_manual(session).await;
        state.configured_sessions.insert(session.to_string());
    }
    let _ = tmux.resize_window(session_window, cols, rows).await;
    state.touched_windows.insert(session_window.to_string());
    state.last_applied = Some((pane_target.to_string(), cols, rows));
}

async fn transition_zoom(tmux: &TmuxClient<'_>, target_pane: &str, zoomed: &mut Option<ZoomState>) {
    if let Some(current) = zoomed.as_ref() {
        if current.pane_target == target_pane {
            return;
        }
        unzoom_if_owned(tmux, current).await;
        *zoomed = None;
    }

    match tmux.is_pane_zoomed(target_pane).await {
        Ok(true) => {
            *zoomed = Some(ZoomState {
                pane_target: target_pane.to_string(),
                we_zoomed_it: false,
            });
        }
        Ok(false) => {
            if tmux.toggle_pane_zoom(target_pane).await.is_ok() {
                *zoomed = Some(ZoomState {
                    pane_target: target_pane.to_string(),
                    we_zoomed_it: true,
                });
            }
        }
        Err(_) => {}
    }
}

async fn unzoom_if_owned(tmux: &TmuxClient<'_>, zoom: &ZoomState) {
    if !zoom.we_zoomed_it {
        return;
    }
    if let Ok(true) = tmux.is_pane_zoomed(&zoom.pane_target).await {
        let _ = tmux.toggle_pane_zoom(&zoom.pane_target).await;
    }
}

async fn restore_windows(tmux: &TmuxClient<'_>, state: &ResizeState) {
    if let Some(zoom) = state.zoomed.as_ref() {
        unzoom_if_owned(tmux, zoom).await;
    }

    let mut by_session: HashMap<String, Vec<String>> = HashMap::new();
    for window in &state.touched_windows {
        let session = match window.split(':').next() {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        by_session.entry(session).or_default().push(window.clone());
    }

    let resize_futures = by_session.into_iter().map(|(session, windows)| async move {
        let dims = tmux.get_client_size(&session).await.ok().flatten();
        if let Some((w, h)) = dims {
            futures::future::join_all(
                windows
                    .iter()
                    .map(|window| tmux.resize_window(window, w, h)),
            )
            .await;
        }
    });
    futures::future::join_all(resize_futures).await;

    futures::future::join_all(
        state
            .configured_sessions
            .iter()
            .map(|session| tmux.unset_window_size(session)),
    )
    .await;
}
