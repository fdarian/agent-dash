use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;

use super::Message;
use crate::claude_transcript::TranscriptCache;
use crate::multiplexer::tmux::TmuxClient;
use crate::sessions::discover::discover_live;
use crate::sessions::live::{detect_prompt_state, LiveSet, LiveStatus, PromptState};

const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

pub(super) fn spawn(poll_tx: mpsc::UnboundedSender<Message>) {
    tokio::spawn(async move {
        let tmux = TmuxClient;
        let mut transcript_cache = TranscriptCache::default();
        let mut unreachable_reported = false;
        loop {
            // An unreachable tmux is reported as `None` so the receiver can tell it apart from
            // "reachable, no agent panes".
            let discovered = match discover_live(&tmux).await {
                Ok(mut live) => {
                    enrich(&mut live, &mut transcript_cache).await;
                    Some(live)
                }
                Err(err) => {
                    // One toast per failure streak; the next success re-arms it.
                    if !unreachable_reported {
                        unreachable_reported = true;
                        let _ = poll_tx.send(Message::TmuxUnreachable(format!("{err:#}")));
                    }
                    None
                }
            };
            if discovered.is_some() {
                unreachable_reported = false;
            }
            let _ = poll_tx.send(Message::SessionsUpdated(discovered));
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    });
}

async fn enrich(live: &mut LiveSet, transcript_cache: &mut TranscriptCache) {
    for session in live.values_mut() {
        // tmux window_activity moves on any window output, so the transcript's last message
        // time is the only activity signal worth using.
        session.last_activity = transcript_cache.last_activity(&session.session_id, &session.cwd);
        // Claude only writes a name to the pid file for named sessions.
        if session.name.is_none() {
            session.name = transcript_cache.title(&session.session_id, &session.cwd);
        }
    }
    let live_session_ids: HashSet<&str> = live.values().map(|s| s.session_id.as_str()).collect();
    transcript_cache.retain_sessions(&live_session_ids);

    let mut prompt_set = tokio::task::JoinSet::new();
    for session in live.values().filter(|s| s.status != LiveStatus::Busy) {
        let target = session.pane.target.clone();
        let session_id = session.session_id.clone();
        prompt_set.spawn(async move {
            let prompt_state = match crate::multiplexer::tmux::capture_pane_visible(&target).await {
                Ok(text) => detect_prompt_state(&text),
                Err(_) => PromptState::None,
            };
            (session_id, prompt_state)
        });
    }
    let mut prompt_states = HashMap::new();
    while let Some(result) = prompt_set.join_next().await {
        if let Ok((session_id, prompt_state)) = result {
            prompt_states.insert(session_id, prompt_state);
        }
    }
    for session in live.values_mut() {
        if let Some(prompt_state) = prompt_states.remove(&session.session_id) {
            session.prompt_state = prompt_state;
        }
    }
}
