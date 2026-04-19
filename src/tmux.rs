use crate::config::AppConfig;
use crate::session::{parse_session_status, AgentSession};
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
            set.spawn(async move {
                let is_claude = check_for_claude_process(&pid).await;
                (i, is_claude)
            });
        }

        let mut claude_indices = std::collections::HashSet::new();
        while let Some(result) = set.join_next().await {
            if let Ok((i, true)) = result {
                claude_indices.insert(i);
            }
        }

        let sessions = parsed
            .into_iter()
            .enumerate()
            .filter(|(i, _)| claude_indices.contains(i))
            .map(|(_, p)| AgentSession {
                pane_id: p.pane_id,
                pane_target: p.pane_target,
                title: p.pane_title.clone(),
                tmux_session_name: p.tmux_session_name,
                status: parse_session_status(&p.pane_title),
            })
            .collect();

        Ok(sessions)
    }

    pub async fn capture_pane_content(&self, pane_target: &str) -> Result<String> {
        run_command(
            "tmux",
            &["capture-pane", "-e", "-t", pane_target, "-p", "-S", "-"],
        )
        .await
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
}

pub async fn capture_pane_visible(pane_target: &str) -> Result<String> {
    run_command("tmux", &["capture-pane", "-p", "-t", pane_target]).await
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

async fn check_for_claude_process(parent_pid: &str) -> bool {
    if let Ok(comm) = run_command("ps", &["-o", "comm=", "-p", parent_pid]).await {
        if comm.trim().ends_with("claude") {
            return true;
        }
    }

    let children = match run_command("pgrep", &["-P", parent_pid]).await {
        Ok(output) => output,
        Err(_) => return false,
    };

    for child_pid in children.lines().filter(|l| !l.is_empty()) {
        if let Ok(comm) = run_command("ps", &["-o", "comm=", "-p", child_pid]).await {
            if comm.trim().ends_with("claude") {
                return true;
            }
        }
        // Recursive check via Box::pin for async recursion
        if Box::pin(check_for_claude_process(child_pid)).await {
            return true;
        }
    }

    false
}
