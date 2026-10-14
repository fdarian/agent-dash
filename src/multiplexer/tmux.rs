use super::{Multiplexer, Pane, PaneRef};
use crate::config::PreviewScrollMode;
use anyhow::{anyhow, Result};
use tokio::process::Command;

pub struct TmuxClient;

/// The tmux session of a target shaped like `session:window.pane`.
pub fn session_name(target: &str) -> &str {
    target.split(':').next().unwrap_or(target)
}

impl TmuxClient {
    pub async fn capture_pane_content(
        &self,
        pane_target: &str,
        scroll_mode: PreviewScrollMode,
    ) -> Result<String> {
        let args: &[&str] = match scroll_mode {
            PreviewScrollMode::Scrollback => {
                &["capture-pane", "-e", "-t", pane_target, "-p", "-S", "-"]
            }
            PreviewScrollMode::Virtualized => &["capture-pane", "-e", "-t", pane_target, "-p"],
        };
        run_command("tmux", args).await
    }

    pub async fn start_pipe_pane(&self, pane_target: &str, fifo_path: &str) -> Result<()> {
        let cmd = format!("cat >> {}", fifo_path);
        run_command("tmux", &["pipe-pane", "-O", "-t", pane_target, &cmd]).await?;
        Ok(())
    }

    pub async fn stop_pipe_pane(&self, pane_target: &str) -> Result<()> {
        run_command("tmux", &["pipe-pane", "-t", pane_target]).await?;
        Ok(())
    }

    pub async fn switch_to_pane(&self, pane_target: &str) -> Result<()> {
        run_command("tmux", &["switch-client", "-t", pane_target]).await?;
        Ok(())
    }

    pub async fn open_popup(&self, pane_target: &str) -> Result<()> {
        let session = session_name(pane_target);
        // Attach to the session, then navigate to the specific window and pane
        let cmd = format!(
            "env -u TMUX tmux attach-session -t '{}' \\; select-window -t '{}' \\; select-pane -t '{}'",
            session, pane_target, pane_target
        );
        run_command(
            "tmux",
            &["display-popup", "-E", "-w", "90%", "-h", "90%", &cmd],
        )
        .await?;
        Ok(())
    }

    pub async fn get_focused_pane_info(&self) -> Option<(String, String)> {
        let output = run_command(
            "tmux",
            &["display-message", "-p", "#{pane_id}\t#{session_name}"],
        )
        .await
        .ok()?;

        let line = output.trim();
        let mut parts = line.splitn(2, '\t');
        let pane_id = parts.next()?.trim().to_string();
        let tmux_session_name = parts.next()?.trim().to_string();

        if pane_id.is_empty() || tmux_session_name.is_empty() {
            return None;
        }
        Some((pane_id, tmux_session_name))
    }

    pub async fn set_window_size_manual(&self, session: &str) -> Result<()> {
        run_command(
            "tmux",
            &["set-option", "-t", session, "window-size", "manual"],
        )
        .await?;
        Ok(())
    }

    pub async fn resize_window(&self, session_window: &str, cols: u16, rows: u16) -> Result<()> {
        let cols_str = cols.to_string();
        let rows_str = rows.to_string();
        run_command(
            "tmux",
            &[
                "resize-window",
                "-t",
                session_window,
                "-x",
                &cols_str,
                "-y",
                &rows_str,
            ],
        )
        .await?;
        Ok(())
    }

    pub async fn unset_window_size(&self, session: &str) -> Result<()> {
        run_command("tmux", &["set-option", "-u", "-t", session, "window-size"]).await?;
        Ok(())
    }

    pub async fn is_pane_zoomed(&self, pane_target: &str) -> Result<bool> {
        let output = run_command(
            "tmux",
            &[
                "display-message",
                "-t",
                pane_target,
                "-p",
                "#{window_zoomed_flag}",
            ],
        )
        .await?;
        Ok(output.trim() == "1")
    }

    pub async fn toggle_pane_zoom(&self, pane_target: &str) -> Result<()> {
        run_command("tmux", &["resize-pane", "-Z", "-t", pane_target]).await?;
        Ok(())
    }

    pub async fn get_window_size(&self, session_window: &str) -> Result<Option<(u16, u16)>> {
        let output = run_command(
            "tmux",
            &[
                "display-message",
                "-t",
                session_window,
                "-p",
                "#{window_width}x#{window_height}",
            ],
        )
        .await?;
        let trimmed = output.trim();
        let (w, h) = match trimmed.split_once('x') {
            Some(pair) => pair,
            None => return Ok(None),
        };
        let cols: u16 = w.parse()?;
        let rows: u16 = h.parse()?;
        if cols == 0 || rows == 0 {
            return Ok(None);
        }
        Ok(Some((cols, rows)))
    }
}

impl Multiplexer for TmuxClient {
    async fn list_panes(&self) -> Result<Vec<Pane>> {
        let format = "#{pane_id}\t#{pane_pid}\t#{session_name}:#{window_index}.#{pane_index}";
        let output = run_command("tmux", &["list-panes", "-a", "-F", format]).await?;

        let mut panes = Vec::new();
        for line in output.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 3 {
                continue;
            }
            let root_pid = parts[1]
                .parse()
                .map_err(|e| anyhow!("tmux list-panes returned pane_pid {:?}: {e}", parts[1]))?;
            panes.push(Pane {
                pane_ref: PaneRef {
                    pane_id: parts[0].to_string(),
                    target: parts[2].to_string(),
                },
                root_pid,
            });
        }
        Ok(panes)
    }

    async fn focus(&self, pane: &PaneRef) -> Result<()> {
        self.switch_to_pane(&pane.target).await
    }

    async fn spawn(&self, name: &str, cwd: &str, command: &str) -> Result<PaneRef> {
        // No shell-command argument: the pane runs the default interactive shell (so rc files
        // apply) and survives the command exiting. The command is typed into it instead.
        let output = run_command(
            "tmux",
            &[
                "new-session",
                "-d",
                "-s",
                name,
                "-c",
                cwd,
                "-P",
                "-F",
                PANE_FORMAT,
            ],
        )
        .await?;
        let created = parse_created_pane(&output)
            .ok_or_else(|| anyhow!("tmux new-session returned unparseable output: {output:?}"))?;
        if let Err(err) = type_command(&created.pane_id, command).await {
            // Best effort: the typing error is the one worth reporting.
            let _ = run_command("tmux", &["kill-session", "-t", &format!("={name}")]).await;
            return Err(err);
        }
        Ok(PaneRef {
            pane_id: created.pane_id,
            target: created.pane_target,
        })
    }

    async fn kill(&self, pane: &PaneRef) -> Result<()> {
        run_command("tmux", &["kill-pane", "-t", &pane.target]).await?;
        Ok(())
    }

    async fn name_taken(&self, name: &str) -> Result<bool> {
        // `=` makes tmux match the name exactly instead of by prefix.
        let target = format!("={name}");
        let status = Command::new("tmux")
            .args(["has-session", "-t", &target])
            .output()
            .await
            .map_err(|e| anyhow!("tmux has-session -t {target}: {e}"))?
            .status;
        Ok(status.success())
    }
}

async fn type_command(pane_id: &str, command: &str) -> Result<()> {
    run_command("tmux", &["send-keys", "-t", pane_id, "-l", "--", command]).await?;
    run_command("tmux", &["send-keys", "-t", pane_id, "Enter"]).await?;
    Ok(())
}

pub async fn capture_pane_visible(pane_target: &str) -> Result<String> {
    run_command("tmux", &["capture-pane", "-p", "-t", pane_target]).await
}

pub async fn send_scroll_up(pane_target: &str, col: u16, row: u16) -> Result<()> {
    let seq = format!("\x1b[<64;{col};{row}M");
    run_command("tmux", &["send-keys", "-l", "-t", pane_target, &seq]).await?;
    Ok(())
}

pub async fn send_scroll_down(pane_target: &str, col: u16, row: u16) -> Result<()> {
    let seq = format!("\x1b[<65;{col};{row}M");
    run_command("tmux", &["send-keys", "-l", "-t", pane_target, &seq]).await?;
    Ok(())
}

const PANE_FORMAT: &str =
    "#{pane_id}\t#{pane_title}\t#{session_name}:#{window_index}.#{pane_index}";

fn parse_created_pane(output: &str) -> Option<CreatedPaneInfo> {
    let parts: Vec<&str> = output.trim().split('\t').collect();
    if parts.len() < 3 || parts[2].split(':').next().is_none_or(str::is_empty) {
        return None;
    }
    Some(CreatedPaneInfo {
        pane_id: parts[0].to_string(),
        pane_target: parts[2].to_string(),
    })
}

pub struct CreatedPaneInfo {
    pub pane_id: String,
    pub pane_target: String,
}

async fn run_command(cmd: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .await
        .map_err(|e| anyhow!("{} {}: {}", cmd, args.join(" "), e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // The subcommand only: the full argument list (formats, targets) buries the reason in a
        // toast.
        let subcommand = args.first().expect("commands are run with a subcommand");
        return Err(anyhow!("{cmd} {subcommand} failed: {}", stderr.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_name_is_the_part_before_the_window() {
        assert_eq!(session_name("work:1.0"), "work");
        assert_eq!(session_name("work"), "work");
    }

    #[test]
    fn parse_created_pane_reads_id_and_target() {
        let pane = parse_created_pane("%3\ttitle\twork:0.0\n").unwrap();
        assert_eq!(pane.pane_id, "%3");
        assert_eq!(pane.pane_target, "work:0.0");
        assert!(parse_created_pane("%3\ttitle").is_none());
    }
}
