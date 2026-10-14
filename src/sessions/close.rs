use super::live::Probe;
use crate::multiplexer::{Multiplexer, PaneRef};
use anyhow::{anyhow, Result};
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// How long each step waits for the Claude process to die before moving on.
struct Waits {
    after_sigterm: Duration,
    after_pane_kill: Duration,
    after_sigkill: Duration,
}

const WAITS: Waits = Waits {
    after_sigterm: Duration::from_secs(3),
    after_pane_kill: Duration::from_secs(1),
    after_sigkill: Duration::from_secs(1),
};

#[derive(Debug)]
pub enum CloseError {
    /// The Claude process is still running, so the session isn't closed.
    ClaudeAlive(anyhow::Error),
    /// Claude is gone but its pane couldn't be removed.
    PaneKept(anyhow::Error),
}

/// Stops Claude and removes its pane. `Ok` means the Claude process is dead and the pane gone.
///
/// SIGTERM first; the pane is removed once Claude exited or 3s passed (this also cleans up the
/// shell `spawn` typed the command into); a process that outlives that gets SIGKILL.
pub async fn close_session<M: Multiplexer>(
    multiplexer: &M,
    probe: &impl Probe,
    claude_pid: u32,
    pane: &PaneRef,
    fallback_session: Option<&str>,
) -> Result<(), CloseError> {
    close_within(
        multiplexer,
        probe,
        claude_pid,
        pane,
        fallback_session,
        &WAITS,
    )
    .await
}

async fn close_within<M: Multiplexer>(
    multiplexer: &M,
    probe: &impl Probe,
    claude_pid: u32,
    pane: &PaneRef,
    fallback_session: Option<&str>,
    waits: &Waits,
) -> Result<(), CloseError> {
    signal(claude_pid, libc::SIGTERM).map_err(CloseError::ClaudeAlive)?;
    wait_for_exit(probe, claude_pid, waits.after_sigterm).await;

    // Not returned yet: whether Claude died decides if this is a failed close.
    let pane_result = multiplexer.kill(pane, fallback_session).await;

    if !wait_for_exit(probe, claude_pid, waits.after_pane_kill).await {
        signal(claude_pid, libc::SIGKILL).map_err(CloseError::ClaudeAlive)?;
        if !wait_for_exit(probe, claude_pid, waits.after_sigkill).await {
            return Err(CloseError::ClaudeAlive(anyhow!(
                "process {claude_pid} is still running after SIGKILL"
            )));
        }
    }

    pane_result.map_err(|err| CloseError::PaneKept(anyhow!("couldn't remove the pane: {err:#}")))
}

/// Whether the process is dead, polling until it is or `timeout` passes.
async fn wait_for_exit(probe: &impl Probe, pid: u32, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    while probe.pid_alive(pid) {
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    true
}

fn signal(pid: u32, signal: libc::c_int) -> Result<()> {
    // 0 and negative values address process groups, not a process.
    if pid == 0 || pid > i32::MAX as u32 {
        return Err(anyhow!("refusing to signal invalid pid {pid}"));
    }
    // SAFETY: sends a signal to one process; no memory is touched.
    let result = unsafe { libc::kill(pid as libc::pid_t, signal) };
    if result == 0 {
        return Ok(());
    }
    let err = std::io::Error::last_os_error();
    // Already gone, which is what the signal was for.
    if err.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(anyhow!("couldn't stop process {pid}: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude_sessions::FsProbe;
    use crate::multiplexer::Pane;
    use crate::sessions::live::PidFileState;
    use std::sync::Mutex;
    use std::time::Instant;
    use tokio::process::Command;

    #[derive(Default)]
    struct RecordingMultiplexer {
        killed: Mutex<Vec<(String, Option<String>)>>,
        kill_fails: bool,
    }

    impl Multiplexer for RecordingMultiplexer {
        async fn list_panes(&self) -> Result<Vec<Pane>> {
            unreachable!()
        }
        async fn focus(&self, _: &PaneRef) -> Result<()> {
            unreachable!()
        }
        async fn spawn(&self, _: &str, _: &str, _: &str) -> Result<PaneRef> {
            unreachable!()
        }
        async fn kill(&self, pane: &PaneRef, fallback_session: Option<&str>) -> Result<()> {
            self.killed
                .lock()
                .unwrap()
                .push((pane.pane_id.clone(), fallback_session.map(str::to_string)));
            if self.kill_fails {
                return Err(anyhow!("no such pane"));
            }
            Ok(())
        }
        async fn name_taken(&self, _: &str) -> Result<bool> {
            unreachable!()
        }
    }

    /// Reports every process as running, whatever signals it got.
    struct AlwaysAlive;

    impl Probe for AlwaysAlive {
        fn pid_file(&self, _: u32) -> PidFileState {
            unreachable!()
        }
        fn pid_alive(&self, _: u32) -> bool {
            true
        }
        fn has_transcript(&self, _: &str) -> bool {
            unreachable!()
        }
    }

    const SHORT_WAITS: Waits = Waits {
        after_sigterm: Duration::from_millis(300),
        after_pane_kill: Duration::from_millis(300),
        after_sigkill: Duration::from_millis(300),
    };

    fn pane() -> PaneRef {
        PaneRef {
            pane_id: "%1".to_string(),
            target: "work:0.0".to_string(),
        }
    }

    /// Reaps the child as soon as it exits: a zombie would still look alive to `pid_alive`.
    fn spawn_reaped(script: &str) -> u32 {
        let mut child = Command::new("sh").args(["-c", script]).spawn().unwrap();
        let pid = child.id().unwrap();
        tokio::spawn(async move { child.wait().await });
        pid
    }

    async fn spawn_ignoring_sigterm() -> u32 {
        // SIGTERM sent before the trap is installed would just kill the shell.
        let ready =
            std::env::temp_dir().join(format!("agent-dash-close-test-{}", std::process::id()));
        let pid = spawn_reaped(&format!(
            "trap '' TERM; touch {}; while :; do sleep 1; done",
            ready.display()
        ));
        while !ready.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        std::fs::remove_file(&ready).unwrap();
        pid
    }

    async fn exited_pid() -> u32 {
        let mut child = Command::new("true").spawn().unwrap();
        let pid = child.id().unwrap();
        child.wait().await.unwrap();
        pid
    }

    #[tokio::test]
    async fn removes_the_pane_as_soon_as_the_process_exits() {
        let pid = spawn_reaped("sleep 30");
        let multiplexer = RecordingMultiplexer::default();

        let started = Instant::now();
        close_session(&multiplexer, &FsProbe, pid, &pane(), Some("main"))
            .await
            .unwrap();

        assert!(started.elapsed() < WAITS.after_sigterm);
        assert!(!FsProbe.pid_alive(pid));
        assert_eq!(
            *multiplexer.killed.lock().unwrap(),
            [("%1".to_string(), Some("main".to_string()))]
        );
    }

    #[tokio::test]
    async fn escalates_to_sigkill_when_the_process_ignores_sigterm() {
        let pid = spawn_ignoring_sigterm().await;
        let multiplexer = RecordingMultiplexer::default();

        let started = Instant::now();
        close_within(&multiplexer, &FsProbe, pid, &pane(), None, &SHORT_WAITS)
            .await
            .unwrap();

        assert!(started.elapsed() >= SHORT_WAITS.after_sigterm);
        assert!(!FsProbe.pid_alive(pid));
        assert_eq!(multiplexer.killed.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_process_that_survives_sigkill_is_a_failed_close() {
        let pid = exited_pid().await;
        let multiplexer = RecordingMultiplexer::default();

        let err = close_within(&multiplexer, &AlwaysAlive, pid, &pane(), None, &SHORT_WAITS)
            .await
            .unwrap_err();

        assert!(matches!(err, CloseError::ClaudeAlive(_)));
        assert!(format!("{err:?}").contains("still running after SIGKILL"));
        // The pane is still attempted, since the process may be on its way out.
        assert_eq!(multiplexer.killed.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_pane_that_stays_after_claude_died_is_not_a_failed_close() {
        let pid = spawn_reaped("sleep 30");
        let multiplexer = RecordingMultiplexer {
            kill_fails: true,
            ..Default::default()
        };

        let err = close_session(&multiplexer, &FsProbe, pid, &pane(), None)
            .await
            .unwrap_err();

        assert!(!FsProbe.pid_alive(pid));
        assert!(
            matches!(&err, CloseError::PaneKept(inner) if inner.to_string().contains("no such pane"))
        );
    }

    #[tokio::test]
    async fn an_already_gone_process_still_gets_its_pane_removed() {
        let pid = exited_pid().await;
        let multiplexer = RecordingMultiplexer::default();

        close_session(&multiplexer, &FsProbe, pid, &pane(), None)
            .await
            .unwrap();

        assert_eq!(multiplexer.killed.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn refuses_pids_that_address_process_groups() {
        let multiplexer = RecordingMultiplexer::default();
        let err = close_session(&multiplexer, &FsProbe, 0, &pane(), None)
            .await
            .unwrap_err();
        assert!(
            matches!(&err, CloseError::ClaudeAlive(inner) if inner.to_string().contains("invalid pid"))
        );
        assert!(multiplexer.killed.lock().unwrap().is_empty());
    }
}
