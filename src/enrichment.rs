use crate::session::{Agent, SessionStatus};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EnrichmentStatus {
    Busy,
    Idle,
}

#[derive(Debug, Deserialize)]
pub struct Enrichment {
    pub agent: Agent,
    pub session_id: Option<String>,
    pub status: Option<EnrichmentStatus>,
    pub cwd: Option<String>,
    pub title: Option<String>,
    pub model: Option<String>,
    pub agent_role: Option<String>,
    #[allow(dead_code)]
    pub updated_at: Option<String>,
}

impl Enrichment {
    pub fn status_as_session_status(&self) -> Option<SessionStatus> {
        self.status.as_ref().map(|s| match s {
            EnrichmentStatus::Busy => SessionStatus::Active,
            EnrichmentStatus::Idle => SessionStatus::Idle,
        })
    }
}

pub fn enrichment_dir() -> PathBuf {
    dirs::home_dir()
        .expect("home directory not found")
        .join(".config/agent-dash/panes")
}

pub fn read(pane_id: &str) -> Option<Enrichment> {
    let path = enrichment_dir().join(format!("{}.json", pane_id));
    let content = std::fs::read_to_string(&path).ok()?;
    // Silently discard malformed files — caller falls back to scraped values
    serde_json::from_str::<Enrichment>(&content).ok()
}

#[allow(dead_code)]
pub fn list_pane_ids() -> Vec<String> {
    let dir = enrichment_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().into_string().ok()?;
            name.strip_suffix(".json").map(str::to_string)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Option<Enrichment> {
        serde_json::from_str::<Enrichment>(json).ok()
    }

    #[test]
    fn test_read_valid_enrichment() {
        let enrichment = parse(
            r#"{
                "agent": "opencode",
                "session_id": "ses_abc123",
                "status": "busy",
                "cwd": "/home/user/project",
                "title": "My Project",
                "model": "gpt-4",
                "agent_role": "Build"
            }"#,
        )
        .unwrap();
        assert!(matches!(enrichment.agent, Agent::Opencode));
        assert_eq!(enrichment.session_id.as_deref(), Some("ses_abc123"));
        assert!(matches!(
            enrichment.status_as_session_status(),
            Some(SessionStatus::Active)
        ));
        assert_eq!(enrichment.cwd.as_deref(), Some("/home/user/project"));
        assert_eq!(enrichment.title.as_deref(), Some("My Project"));
        assert_eq!(enrichment.model.as_deref(), Some("gpt-4"));
        assert_eq!(enrichment.agent_role.as_deref(), Some("Build"));
    }

    #[test]
    fn test_read_partial_enrichment() {
        let enrichment = parse(r#"{"agent": "claude", "session_id": "ses_xyz"}"#).unwrap();
        assert!(matches!(enrichment.agent, Agent::Claude));
        assert_eq!(enrichment.session_id.as_deref(), Some("ses_xyz"));
        assert!(enrichment.status.is_none());
        assert!(enrichment.cwd.is_none());
    }

    #[test]
    fn test_malformed_json_returns_none() {
        assert!(parse("not valid json {{{").is_none());
    }

    #[test]
    fn test_missing_agent_field_returns_none() {
        assert!(parse(r#"{"session_id": "ses_abc"}"#).is_none());
    }

    #[test]
    fn test_missing_file_returns_none() {
        // Use a path that is guaranteed not to exist
        let path = std::env::temp_dir().join("agent-dash-test-does-not-exist-xyz.json");
        let result = std::fs::read_to_string(&path)
            .ok()
            .and_then(|c| serde_json::from_str::<Enrichment>(&c).ok());
        assert!(result.is_none());
    }

    #[test]
    fn test_idle_status_maps_to_idle() {
        let enrichment = parse(r#"{"agent": "opencode", "status": "idle"}"#).unwrap();
        assert!(matches!(
            enrichment.status_as_session_status(),
            Some(SessionStatus::Idle)
        ));
    }
}
