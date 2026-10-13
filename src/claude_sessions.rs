use crate::session::SessionStatus;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClaudeStatus {
    Busy,
    Idle,
    /// A status Claude added after this was written. Must not override the scraped status.
    #[serde(other)]
    Unknown,
}

impl ClaudeStatus {
    pub fn as_session_status(&self) -> Option<SessionStatus> {
        match self {
            ClaudeStatus::Busy => Some(SessionStatus::Active),
            ClaudeStatus::Idle => Some(SessionStatus::Idle),
            ClaudeStatus::Unknown => None,
        }
    }
}

/// Subset of `~/.claude/sessions/<pid>.json`, which Claude Code writes for each live process.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeSession {
    pub session_id: String,
    pub cwd: String,
    pub status: ClaudeStatus,
}

fn sessions_dir() -> PathBuf {
    dirs::home_dir()
        .expect("home directory not found")
        .join(".claude/sessions")
}

fn parse(json: &str) -> Option<ClaudeSession> {
    serde_json::from_str::<ClaudeSession>(json).ok()
}

/// Missing files are normal (older Claude versions); malformed ones are ignored the same way
/// so the caller falls back to scraped values.
pub fn read(pid: u32) -> Option<ClaudeSession> {
    let content = std::fs::read_to_string(sessions_dir().join(format!("{}.json", pid))).ok()?;
    parse(&content)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL_EXAMPLE: &str = r#"{"pid":89544,"sessionId":"aa4e035a-0273-4e80-8b10-6b39172ebc2e","cwd":"/Users/farreldarian/code/fdarian/agent-dash","startedAt":1791823919880,"procStart":"Mon Oct 12 16:51:58 2026","version":"2.1.296","kind":"interactive","entrypoint":"cli","tmux":"/Users/farreldarian:@80.%179","name":"agent-dash-72","status":"busy","updatedAt":1791823981701,"statusUpdatedAt":1791823981701}"#;

    #[test]
    fn test_parse_real_example() {
        let session = parse(REAL_EXAMPLE).unwrap();
        assert_eq!(session.session_id, "aa4e035a-0273-4e80-8b10-6b39172ebc2e");
        assert_eq!(session.cwd, "/Users/farreldarian/code/fdarian/agent-dash");
        assert_eq!(session.status, ClaudeStatus::Busy);
        assert!(matches!(
            session.status.as_session_status(),
            Some(SessionStatus::Active)
        ));
    }

    #[test]
    fn test_parse_idle() {
        let session = parse(r#"{"sessionId":"s1","cwd":"/tmp","status":"idle"}"#).unwrap();
        assert_eq!(session.status, ClaudeStatus::Idle);
        assert!(matches!(
            session.status.as_session_status(),
            Some(SessionStatus::Idle)
        ));
    }

    #[test]
    fn test_unknown_status_does_not_override() {
        let session = parse(r#"{"sessionId":"s1","cwd":"/tmp","status":"waiting"}"#).unwrap();
        assert_eq!(session.status, ClaudeStatus::Unknown);
        assert!(session.status.as_session_status().is_none());
    }

    #[test]
    fn test_missing_fields_returns_none() {
        assert!(parse(r#"{"cwd":"/tmp","status":"busy"}"#).is_none());
        assert!(parse(r#"{"sessionId":"s1","status":"busy"}"#).is_none());
        assert!(parse(r#"{"sessionId":"s1","cwd":"/tmp"}"#).is_none());
    }

    #[test]
    fn test_malformed_json_returns_none() {
        assert!(parse("not valid json {{{").is_none());
        assert!(parse("").is_none());
    }

    #[test]
    fn test_missing_file_returns_none() {
        assert!(read(u32::MAX).is_none());
    }
}
