pub mod naming;
pub mod tmux;

use anyhow::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneRef {
    pub pane_id: String,
    pub target: String,
}

/// A pane as the multiplexer reports it, with no knowledge of what runs inside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    pub pane_ref: PaneRef,
    /// The process the pane started; whatever the user runs is that process or a descendant.
    pub root_pid: u32,
}

impl PaneRef {
    /// The multiplexer-level session the pane belongs to. Tmux targets look like
    /// `session:window.pane`.
    pub fn session_name(&self) -> &str {
        tmux::session_name(&self.target)
    }
}

/// Pane lifecycle only. Preview capture, pipes, resize and copy mode stay
/// tmux-specific on purpose.
pub trait Multiplexer {
    /// `Err` means the multiplexer couldn't be reached, which is different from reaching it
    /// and finding no panes.
    async fn list_panes(&self) -> Result<Vec<Pane>>;
    async fn focus(&self, pane: &PaneRef) -> Result<()>;
    async fn spawn(&self, name: &str, cwd: &str, command: &str) -> Result<PaneRef>;
    /// Removes the pane. When it is the last one of its multiplexer session, clients attached
    /// to that session are first moved to another one (`fallback_session` if it exists) so
    /// they aren't kicked out.
    async fn kill(&self, pane: &PaneRef, fallback_session: Option<&str>) -> Result<()>;
    async fn name_taken(&self, name: &str) -> Result<bool>;
}
