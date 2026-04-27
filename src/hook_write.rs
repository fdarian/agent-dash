use crate::enrichment::{enrichment_dir, Enrichment};
use serde::Serialize;
use std::io::Read;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    SessionStart,
    PromptSubmit,
    Stop,
    SessionEnd,
}

impl EventKind {
    pub fn from_str(s: &str) -> Option<EventKind> {
        match s {
            "session-start" => Some(EventKind::SessionStart),
            "prompt-submit" => Some(EventKind::PromptSubmit),
            "stop" => Some(EventKind::Stop),
            "session-end" => Some(EventKind::SessionEnd),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize)]
struct EnrichmentWrite<'a> {
    agent: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cwd: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
    updated_at: &'a str,
}

/// Core logic, separated for testability. `base_dir` is the panes directory.
pub fn run(
    event: EventKind,
    pane_id: &str,
    stdin_json: &str,
    base_dir: &std::path::Path,
) -> anyhow::Result<()> {
    let target = base_dir.join(format!("{}.json", pane_id));

    if event == EventKind::SessionEnd {
        match std::fs::remove_file(&target) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        return Ok(());
    }

    let payload: serde_json::Value = serde_json::from_str(stdin_json)?;

    // Determine status from event kind.
    let status_str = match event {
        EventKind::PromptSubmit => "busy",
        EventKind::SessionStart | EventKind::Stop => "idle",
        EventKind::SessionEnd => unreachable!(),
    };

    // Read existing enrichment to preserve fields that only arrive on SessionStart
    // (model, session_id). Without this, a Stop event would wipe model from the file.
    let existing = std::fs::read_to_string(&target)
        .ok()
        .and_then(|c| serde_json::from_str::<Enrichment>(&c).ok());

    let payload_session_id = payload.get("session_id").and_then(|v| v.as_str());
    let payload_model = payload.get("model").and_then(|v| v.as_str());
    let payload_cwd = payload.get("cwd").and_then(|v| v.as_str());

    // Prefer the current payload value; fall back to whatever was persisted.
    let session_id: Option<String> = payload_session_id
        .map(str::to_string)
        .or_else(|| existing.as_ref().and_then(|e| e.session_id.clone()));
    let model: Option<String> = payload_model
        .map(str::to_string)
        .or_else(|| existing.as_ref().and_then(|e| e.model.clone()));
    let cwd: Option<String> = payload_cwd
        .map(str::to_string)
        .or_else(|| existing.as_ref().and_then(|e| e.cwd.clone()));

    let now = chrono::Utc::now().to_rfc3339();

    let write = EnrichmentWrite {
        agent: "claude",
        session_id: session_id.as_deref(),
        status: Some(status_str),
        cwd: cwd.as_deref(),
        model: model.as_deref(),
        updated_at: &now,
    };

    let json = serde_json::to_string_pretty(&write)?;

    std::fs::create_dir_all(base_dir)?;

    let tmp = target.with_extension("json.tmp");
    std::fs::write(&tmp, &json)?;
    std::fs::rename(&tmp, &target)?;

    Ok(())
}

/// Entry point called from main. Reads env + stdin, then delegates to `run`.
pub fn execute(event: EventKind) {
    let pane_id = match std::env::var("TMUX_PANE").ok().filter(|s| !s.is_empty()) {
        Some(id) => id,
        None => {
            // Not inside tmux — genuine no-op.
            return;
        }
    };

    let mut stdin_json = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut stdin_json) {
        eprintln!("agent-dash hook-write: failed to read stdin: {}", e);
        return;
    }

    let base_dir = enrichment_dir();
    if let Err(e) = run(event, &pane_id, &stdin_json, &base_dir) {
        eprintln!("agent-dash hook-write: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Agent;

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("agent-dash-test-hook-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn session_start_writes_enrichment() {
        let dir = temp_dir().join("session_start");
        std::fs::create_dir_all(&dir).unwrap();
        let pane_id = "test-pane-1";
        let payload = r#"{
            "session_id": "ses_abc123",
            "cwd": "/tmp/myproject",
            "model": "claude-sonnet-4-6",
            "hook_event_name": "SessionStart"
        }"#;

        run(EventKind::SessionStart, pane_id, payload, &dir).unwrap();

        // Re-read via the enrichment module's read() logic directly from file.
        let content = std::fs::read_to_string(dir.join("test-pane-1.json")).unwrap();
        let enrichment: crate::enrichment::Enrichment = serde_json::from_str(&content).unwrap();

        assert!(matches!(enrichment.agent, Agent::Claude));
        assert_eq!(enrichment.session_id.as_deref(), Some("ses_abc123"));
        assert_eq!(enrichment.cwd.as_deref(), Some("/tmp/myproject"));
        assert_eq!(enrichment.model.as_deref(), Some("claude-sonnet-4-6"));
        assert!(matches!(
            enrichment.status,
            Some(crate::enrichment::EnrichmentStatus::Idle)
        ));
    }

    #[test]
    fn prompt_submit_sets_busy() {
        let dir = temp_dir().join("prompt_submit");
        std::fs::create_dir_all(&dir).unwrap();
        let pane_id = "test-pane-2";
        let payload = r#"{
            "session_id": "ses_xyz",
            "cwd": "/tmp",
            "hook_event_name": "UserPromptSubmit",
            "prompt": "hello"
        }"#;

        run(EventKind::PromptSubmit, pane_id, payload, &dir).unwrap();

        let content = std::fs::read_to_string(dir.join("test-pane-2.json")).unwrap();
        let enrichment: crate::enrichment::Enrichment = serde_json::from_str(&content).unwrap();

        assert!(matches!(
            enrichment.status,
            Some(crate::enrichment::EnrichmentStatus::Busy)
        ));
    }

    #[test]
    fn stop_preserves_model_from_prior_write() {
        let dir = temp_dir().join("stop_preserve");
        std::fs::create_dir_all(&dir).unwrap();
        let pane_id = "test-pane-3";

        // Simulate a prior SessionStart write that includes model.
        let start_payload = r#"{
            "session_id": "ses_preserve",
            "cwd": "/home/user",
            "model": "claude-opus-4",
            "hook_event_name": "SessionStart"
        }"#;
        run(EventKind::SessionStart, pane_id, start_payload, &dir).unwrap();

        // Stop event doesn't carry model.
        let stop_payload = r#"{
            "session_id": "ses_preserve",
            "cwd": "/home/user",
            "hook_event_name": "Stop",
            "stop_hook_active": false
        }"#;
        run(EventKind::Stop, pane_id, stop_payload, &dir).unwrap();

        let content = std::fs::read_to_string(dir.join("test-pane-3.json")).unwrap();
        let enrichment: crate::enrichment::Enrichment = serde_json::from_str(&content).unwrap();

        assert_eq!(
            enrichment.model.as_deref(),
            Some("claude-opus-4"),
            "model must survive a Stop event that doesn't include it"
        );
        assert!(matches!(
            enrichment.status,
            Some(crate::enrichment::EnrichmentStatus::Idle)
        ));
    }

    #[test]
    fn session_end_deletes_file() {
        let dir = temp_dir().join("session_end");
        std::fs::create_dir_all(&dir).unwrap();
        let pane_id = "test-pane-4";

        // Pre-create a file.
        let path = dir.join("test-pane-4.json");
        std::fs::write(&path, r#"{"agent":"claude"}"#).unwrap();

        let end_payload =
            r#"{"session_id":"ses_gone","hook_event_name":"SessionEnd","reason":"other"}"#;
        run(EventKind::SessionEnd, pane_id, end_payload, &dir).unwrap();

        assert!(
            !path.exists(),
            "enrichment file should be deleted on SessionEnd"
        );
    }

    #[test]
    fn session_end_tolerates_missing_file() {
        let dir = temp_dir().join("session_end_missing");
        std::fs::create_dir_all(&dir).unwrap();
        // No file created — should not error.
        let result = run(
            EventKind::SessionEnd,
            "nonexistent-pane",
            r#"{"hook_event_name":"SessionEnd"}"#,
            &dir,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn roundtrip_deserializes_via_enrichment_read() {
        let dir = temp_dir().join("roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let pane_id = "roundtrip-pane";
        let payload = r#"{
            "session_id": "ses_roundtrip",
            "cwd": "/round/trip",
            "model": "claude-haiku-4",
            "hook_event_name": "SessionStart"
        }"#;

        run(EventKind::SessionStart, pane_id, payload, &dir).unwrap();

        // Verify the file can be re-read via the same path the enrichment reader uses.
        let content = std::fs::read_to_string(dir.join("roundtrip-pane.json")).unwrap();
        let enrichment: crate::enrichment::Enrichment = serde_json::from_str(&content).unwrap();

        assert!(matches!(enrichment.agent, Agent::Claude));
        assert_eq!(enrichment.session_id.as_deref(), Some("ses_roundtrip"));
        assert_eq!(enrichment.cwd.as_deref(), Some("/round/trip"));
        assert_eq!(enrichment.model.as_deref(), Some("claude-haiku-4"));
        assert!(enrichment.updated_at.is_some());
    }
}
