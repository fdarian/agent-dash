use std::os::unix::fs::OpenOptionsExt;
use tokio::io::AsyncReadExt;
use tokio::sync::{mpsc, watch};

use crate::app::Message;
use crate::tmux::TmuxClient;

pub struct PipePaneWatcher {
    fifo_path: String,
}

impl PipePaneWatcher {
    pub fn new() -> Self {
        let pid = std::process::id();
        let fifo_path = format!("/tmp/agent-dash-{}-preview.fifo", pid);

        // Remove stale FIFO if it exists (crash recovery)
        let _ = std::fs::remove_file(&fifo_path);

        // Create FIFO via mkfifo command
        let _ = std::process::Command::new("mkfifo")
            .arg(&fifo_path)
            .status();

        Self { fifo_path }
    }

    pub fn fifo_path(&self) -> &str {
        &self.fifo_path
    }

    pub fn cleanup(&mut self) {
        let _ = std::fs::remove_file(&self.fifo_path);
    }
}

impl Drop for PipePaneWatcher {
    fn drop(&mut self) {
        self.cleanup();
    }
}

pub fn spawn_preview_task(
    tx: mpsc::UnboundedSender<Message>,
    mut target_rx: watch::Receiver<Option<String>>,
    fifo_path: String,
) {
    tokio::spawn(async move {
        let config = crate::config::load_config(false);
        let tmux = TmuxClient::new(&config);
        let mut previous_content = String::new();
        let mut current_target: Option<String> = None;

        // Open FIFO with O_RDWR to avoid blocking when no writer is connected
        let fifo_file = match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&fifo_path)
        {
            Ok(f) => f,
            Err(_) => return, // FIFO creation failed, give up
        };
        let mut fifo = tokio::io::BufReader::new(tokio::fs::File::from_std(fifo_file));
        let mut buf = [0u8; 4096];

        let mut debounce: Option<tokio::time::Instant> = None;
        let fallback_interval = tokio::time::Duration::from_secs(2);
        let debounce_duration = tokio::time::Duration::from_millis(50);

        let mut fallback_deadline = tokio::time::Instant::now() + fallback_interval;

        loop {
            let debounce_sleep = match debounce {
                Some(deadline) => tokio::time::sleep_until(deadline),
                None => tokio::time::sleep(tokio::time::Duration::from_secs(86400)),
            };
            let fallback_sleep = tokio::time::sleep_until(fallback_deadline);

            tokio::select! {
                // Target changed
                result = target_rx.changed() => {
                    if result.is_err() {
                        break; // Sender dropped, app is shutting down
                    }
                    let new_target = target_rx.borrow_and_update().clone();

                    // Stop old pipe-pane
                    if let Some(old) = current_target.take() {
                        let _ = tmux.stop_pipe_pane(&old).await;
                    }

                    // Drain FIFO to discard stale data
                    loop {
                        match fifo.read(&mut buf).await {
                            Ok(0) | Err(_) => break,
                            Ok(_) => continue,
                        }
                    }

                    current_target = new_target.clone();
                    previous_content.clear();

                    if let Some(ref target) = current_target {
                        // Immediate capture for new target
                        if let Ok(content) = tmux.capture_pane_content(target).await {
                            previous_content = content.clone();
                            let _ = tx.send(Message::PreviewUpdated(content));
                        }
                        // Start pipe-pane for new target
                        let _ = tmux.start_pipe_pane(target, &fifo_path).await;
                    }

                    debounce = None;
                    fallback_deadline = tokio::time::Instant::now() + fallback_interval;
                }

                // FIFO data available = content changed
                result = fifo.read(&mut buf) => {
                    match result {
                        Ok(0) => {
                            // EOF — writer disconnected, will re-trigger on next write
                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        }
                        Ok(_) => {
                            // Data arrived — reset debounce timer
                            debounce = Some(tokio::time::Instant::now() + debounce_duration);
                        }
                        Err(_) => {
                            // EWOULDBLOCK or other error — no data available, that's fine
                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        }
                    }
                }

                // Debounce fired — capture pane content
                _ = debounce_sleep, if debounce.is_some() => {
                    debounce = None;
                    if let Some(ref target) = current_target {
                        if let Ok(content) = tmux.capture_pane_content(target).await {
                            if content != previous_content {
                                previous_content = content.clone();
                                let _ = tx.send(Message::PreviewUpdated(content));
                            }
                        }
                    }
                    fallback_deadline = tokio::time::Instant::now() + fallback_interval;
                }

                // Fallback poll (safety net)
                _ = fallback_sleep => {
                    if let Some(ref target) = current_target {
                        if let Ok(content) = tmux.capture_pane_content(target).await {
                            if content != previous_content {
                                previous_content = content.clone();
                                let _ = tx.send(Message::PreviewUpdated(content));
                            }
                        }
                    }
                    fallback_deadline = tokio::time::Instant::now() + fallback_interval;
                }
            }
        }
    });
}
