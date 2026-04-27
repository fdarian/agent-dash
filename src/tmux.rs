use crate::config::{AppConfig, PreviewScrollMode};
use crate::session::{parse_session_status, Agent, AgentSession};
use anyhow::{anyhow, Result};
use tokio::process::Command;

pub struct TmuxClient<'a> {
    config: &'a AppConfig,
}

impl<'a> TmuxClient<'a> {
    pub fn new(config: &'a AppConfig) -> Self {
        Self { config }
    }

    pub async fn discover_sessions(&self) -> Result<Vec<AgentSession>> {
        let format =
            "#{pane_id}\t#{pane_pid}\t#{pane_title}\t#{session_name}:#{window_index}.#{pane_index}";
        let output = run_command("tmux", &["list-panes", "-a", "-F", format]).await;

        let output = match output {
            Ok(o) => o,
            Err(_) => return Ok(Vec::new()),
        };

        struct ParsedPane {
            pane_id: String,
            pane_pid: String,
            pane_title: String,
            pane_target: String,
            tmux_session_name: String,
        }

        let mut parsed = Vec::new();
        for line in output.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 4 {
                continue;
            }
            let pane_target = parts[3];
            let tmux_session_name = match pane_target.split(':').next() {
                Some(s) if !s.is_empty() => s,
                _ => continue,
            };
            parsed.push(ParsedPane {
                pane_id: parts[0].to_string(),
                pane_pid: parts[1].to_string(),
                pane_title: parts[2].to_string(),
                pane_target: pane_target.to_string(),
                tmux_session_name: tmux_session_name.to_string(),
            });
        }

        let mut set = tokio::task::JoinSet::new();
        for (i, p) in parsed.iter().enumerate() {
            let pid = p.pane_pid.clone();
            let pane_target = p.pane_target.clone();
            set.spawn(async move {
                let agent = detect_agent(&pid).await;
                let content = if agent == Some(Agent::Opencode) {
                    capture_pane_visible(&pane_target).await.ok()
                } else {
                    None
                };
                (i, agent, content)
            });
        }

        let mut agent_map: std::collections::HashMap<usize, (Agent, Option<String>)> =
            std::collections::HashMap::new();
        while let Some(result) = set.join_next().await {
            if let Ok((i, Some(agent), content)) = result {
                agent_map.insert(i, (agent, content));
            }
        }

        let mut sessions = Vec::new();
        for (i, p) in parsed.into_iter().enumerate() {
            if let Some((agent, content)) = agent_map.remove(&i) {
                let status = parse_session_status(agent, &p.pane_title, content.as_deref());
                sessions.push(AgentSession {
                    pane_id: p.pane_id,
                    pane_target: p.pane_target,
                    title: p.pane_title,
                    tmux_session_name: p.tmux_session_name,
                    status,
                    agent,
                    session_id: None,
                });
            }
        }

        Ok(sessions)
    }

    pub async fn capture_pane_content(&self, pane_target: &str) -> Result<String> {
        let args: &[&str] = match self.config.preview_scroll_mode {
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
        // Extract session name from target (format: "session:window.pane")
        let session = pane_target.split(':').next().unwrap_or(pane_target);
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

    pub async fn create_window(
        &self,
        tmux_session_name: &str,
        cwd: Option<&str>,
    ) -> Result<Option<CreatedPaneInfo>> {
        let format = "#{pane_id}\t#{pane_title}\t#{session_name}:#{window_index}.#{pane_index}";
        let mut args = vec![
            "new-window",
            "-d",
            "-P",
            "-F",
            format,
            "-t",
            tmux_session_name,
        ];
        if let Some(cwd) = cwd {
            args.push("-c");
            args.push(cwd);
        }
        args.push(&self.config.command);

        let output = run_command("tmux", &args).await?;
        let parts: Vec<&str> = output.trim().split('\t').collect();
        if parts.len() < 3 {
            return Ok(None);
        }

        let pane_target = parts[2];
        let tmux_session_name = match pane_target.split(':').next() {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => return Ok(None),
        };

        Ok(Some(CreatedPaneInfo {
            pane_id: parts[0].to_string(),
            pane_title: parts[1].to_string(),
            pane_target: pane_target.to_string(),
            tmux_session_name,
        }))
    }

    pub async fn get_pane_cwd(&self, target: &str) -> Result<String> {
        let output = run_command(
            "tmux",
            &[
                "display-message",
                "-p",
                "-t",
                target,
                "#{pane_current_path}",
            ],
        )
        .await?;
        Ok(output.trim().to_string())
    }

    pub async fn kill_pane(&self, pane_target: &str) -> Result<()> {
        run_command("tmux", &["kill-pane", "-t", pane_target]).await?;
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

pub async fn capture_pane_visible(pane_target: &str) -> Result<String> {
    run_command("tmux", &["capture-pane", "-p", "-t", pane_target]).await
}

pub async fn capture_pane_visible_colored(pane_target: &str) -> Result<String> {
    run_command("tmux", &["capture-pane", "-e", "-p", "-t", pane_target]).await
}

pub async fn send_scroll_up(pane_target: &str) -> Result<()> {
    run_command(
        "tmux",
        &["send-keys", "-l", "-t", pane_target, "\x1b[<64;1;1M"],
    )
    .await?;
    Ok(())
}

pub async fn send_scroll_down(pane_target: &str) -> Result<()> {
    run_command(
        "tmux",
        &["send-keys", "-l", "-t", pane_target, "\x1b[<65;1;1M"],
    )
    .await?;
    Ok(())
}

pub struct CreatedPaneInfo {
    pub pane_id: String,
    pub pane_title: String,
    pub pane_target: String,
    pub tmux_session_name: String,
}

async fn run_command(cmd: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .await
        .map_err(|e| anyhow!("{} {}: {}", cmd, args.join(" "), e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("{} {} failed: {}", cmd, args.join(" "), stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn detect_agent(parent_pid: &str) -> Option<Agent> {
    if let Ok(comm) = run_command("ps", &["-o", "comm=", "-p", parent_pid]).await {
        let trimmed = comm.trim();
        if trimmed.ends_with("claude") {
            return Some(Agent::Claude);
        }
        if trimmed.ends_with("opencode") {
            return Some(Agent::Opencode);
        }
    }

    let children = match run_command("pgrep", &["-P", parent_pid]).await {
        Ok(output) => output,
        Err(_) => return None,
    };

    for child_pid in children.lines().filter(|l| !l.is_empty()) {
        if let Ok(comm) = run_command("ps", &["-o", "comm=", "-p", child_pid]).await {
            let trimmed = comm.trim();
            if trimmed.ends_with("claude") {
                return Some(Agent::Claude);
            }
            if trimmed.ends_with("opencode") {
                return Some(Agent::Opencode);
            }
        }
        // Recursive check via Box::pin for async recursion
        if let Some(agent) = Box::pin(detect_agent(child_pid)).await {
            return Some(agent);
        }
    }

    None
}
