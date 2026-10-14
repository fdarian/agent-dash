use super::live::{LiveSession, LiveSet, PromptState};
use crate::claude_sessions;
use crate::multiplexer::{Multiplexer, Pane};
use anyhow::Result;

/// The Claude sessions running in the multiplexer's panes. `Err` means the multiplexer couldn't
/// be reached, which is different from reaching it and finding no sessions.
pub async fn discover_live<M: Multiplexer>(multiplexer: &M) -> Result<LiveSet> {
    let panes = multiplexer.list_panes().await?;

    // Awaited in pane order so that ties in `LiveSet::collapse` resolve the same way every tick.
    let detections: Vec<_> = panes
        .into_iter()
        .map(|pane| {
            tokio::spawn(async move {
                let claude_pid = claude_sessions::find_claude_pid(pane.root_pid).await;
                (pane, claude_pid)
            })
        })
        .collect();

    let mut sessions = Vec::new();
    for detection in detections {
        // A panicked detection task only loses its own pane for this tick.
        let Ok((pane, Some(claude_pid))) = detection.await else {
            continue;
        };
        if let Some(session) = live_session(pane, claude_pid) {
            sessions.push(session);
        }
    }
    Ok(LiveSet::collapse(sessions))
}

fn live_session(pane: Pane, claude_pid: u32) -> Option<LiveSession> {
    // Without a readable pid file there is no session id to key on. The pane is skipped for
    // this tick only; `track` treats the resulting disappearance as a pending close.
    let claude_sessions::PidFile::Session(claude) = claude_sessions::read(claude_pid) else {
        return None;
    };
    Some(LiveSession {
        session_id: claude.session_id,
        pid: claude_pid,
        pane: pane.pane_ref,
        status: claude.status.as_live_status(),
        prompt_state: PromptState::None,
        name: claude.name,
        cwd: claude.cwd,
        started_at: claude.started_at,
        last_activity: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multiplexer::PaneRef;

    struct FakeMultiplexer(Result<Vec<Pane>, &'static str>);

    impl Multiplexer for FakeMultiplexer {
        async fn list_panes(&self) -> Result<Vec<Pane>> {
            self.0.clone().map_err(|message| anyhow::anyhow!(message))
        }
        async fn focus(&self, _: &PaneRef) -> Result<()> {
            unreachable!()
        }
        async fn spawn(&self, _: &str, _: &str, _: &str) -> Result<PaneRef> {
            unreachable!()
        }
        async fn kill(&self, _: &PaneRef) -> Result<()> {
            unreachable!()
        }
        async fn name_taken(&self, _: &str) -> Result<bool> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn unreachable_multiplexer_keeps_its_error() {
        let err = discover_live(&FakeMultiplexer(Err("no server running")))
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "no server running");
    }

    #[tokio::test]
    async fn panes_without_claude_yield_an_empty_set() {
        let pane = Pane {
            pane_ref: PaneRef {
                pane_id: "%1".to_string(),
                target: "work:0.0".to_string(),
            },
            // Not a pid anything can be running as.
            root_pid: u32::MAX,
        };
        let live = discover_live(&FakeMultiplexer(Ok(vec![pane])))
            .await
            .unwrap();
        assert_eq!(live.values().count(), 0);
    }
}
