use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const CHUNK_SIZE: usize = 64 * 1024;

#[derive(Deserialize)]
struct TranscriptLine {
    #[serde(rename = "type")]
    kind: String,
    timestamp: Option<String>,
}

struct Entry {
    path: PathBuf,
    len: u64,
    modified: SystemTime,
    last_activity: Option<u64>,
}

/// Last-activity times parsed from `~/.claude/projects/**/<session_id>.jsonl`, keyed by session id.
/// A transcript is only re-read when its length or mtime changes.
#[derive(Default)]
pub struct TranscriptCache {
    entries: HashMap<String, Entry>,
}

impl TranscriptCache {
    /// `None` means no readable transcript, so the caller keeps its existing value.
    pub fn last_activity(&mut self, session_id: &str, cwd: &str) -> Option<u64> {
        if !self.entries.contains_key(session_id) {
            let path = resolve_path(&projects_dir(), session_id, cwd)?;
            self.entries.insert(
                session_id.to_string(),
                Entry {
                    path,
                    len: 0,
                    modified: SystemTime::UNIX_EPOCH,
                    last_activity: None,
                },
            );
        }
        let entry = self.entries.get_mut(session_id)?;
        let stat = std::fs::metadata(&entry.path).and_then(|metadata| {
            metadata
                .modified()
                .map(|modified| (metadata.len(), modified))
        });
        let Ok(stat) = stat else {
            // Deleted or moved: drop the cached path so the next tick resolves it again.
            self.entries.remove(session_id);
            return None;
        };
        if stat != (entry.len, entry.modified) {
            let Ok(last_activity) = last_message_timestamp(&entry.path, CHUNK_SIZE) else {
                self.entries.remove(session_id);
                return None;
            };
            entry.len = stat.0;
            entry.modified = stat.1;
            entry.last_activity = last_activity;
        }
        entry.last_activity
    }

    pub fn retain_sessions(&mut self, live_session_ids: &HashSet<&str>) {
        self.entries
            .retain(|session_id, _| live_session_ids.contains(session_id.as_str()));
    }
}

fn projects_dir() -> PathBuf {
    dirs::home_dir()
        .expect("home directory not found")
        .join(".claude/projects")
}

fn encode_cwd(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Claude names the project directory after the encoded cwd, but the cwd may have changed since
/// the session started and long paths are hashed, so fall back to scanning every project.
fn resolve_path(projects_dir: &Path, session_id: &str, cwd: &str) -> Option<PathBuf> {
    let file_name = format!("{}.jsonl", session_id);
    let expected = projects_dir.join(encode_cwd(cwd)).join(&file_name);
    if expected.is_file() {
        return Some(expected);
    }
    std::fs::read_dir(projects_dir)
        .ok()?
        .filter_map(|dir| dir.ok())
        .map(|dir| dir.path().join(&file_name))
        .find(|candidate| candidate.is_file())
}

fn parse_line(line: &[u8]) -> Option<u64> {
    let parsed = serde_json::from_slice::<TranscriptLine>(line).ok()?;
    if parsed.kind != "user" && parsed.kind != "assistant" {
        return None;
    }
    let timestamp = chrono::DateTime::parse_from_rfc3339(&parsed.timestamp?).ok()?;
    u64::try_from(timestamp.timestamp()).ok()
}

/// Walks the file backwards so only the tail is read; transcripts can be many MB and end in
/// metadata lines (`ai-title`, `last-prompt`, ...) that carry no activity time.
fn last_message_timestamp(path: &Path, chunk_size: usize) -> io::Result<Option<u64>> {
    let mut file = File::open(path)?;
    let mut position = file.metadata()?.len();
    // Everything after the chunk being read. Its first line may be cut off by the chunk
    // boundary, so it is only parsed once the preceding chunk supplies the rest.
    let mut carry: Vec<u8> = Vec::new();
    while position > 0 {
        let read_len = position.min(chunk_size as u64);
        position -= read_len;
        let mut buf = vec![0u8; read_len as usize];
        file.seek(SeekFrom::Start(position))?;
        file.read_exact(&mut buf)?;
        buf.extend_from_slice(&carry);

        let complete_start = if position == 0 {
            0
        } else {
            match buf.iter().position(|&b| b == b'\n') {
                Some(newline) => newline + 1,
                None => {
                    carry = buf;
                    continue;
                }
            }
        };
        if let Some(found) = buf[complete_start..]
            .split(|&b| b == b'\n')
            .rev()
            .find_map(parse_line)
        {
            return Ok(Some(found));
        }
        buf.truncate(complete_start);
        carry = buf;
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("agent-dash-{}-{}", name, std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn message(kind: &str, timestamp: &str) -> String {
        format!(
            r#"{{"type":"{}","timestamp":"{}","message":"hello"}}"#,
            kind, timestamp
        )
    }

    fn write_lines(dir: &TempDir, lines: &[String]) -> PathBuf {
        let path = dir.0.join("t.jsonl");
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        path
    }

    fn epoch(rfc3339: &str) -> u64 {
        chrono::DateTime::parse_from_rfc3339(rfc3339)
            .unwrap()
            .timestamp() as u64
    }

    #[test]
    fn test_encode_cwd() {
        assert_eq!(
            encode_cwd("/Users/x/agent-dash/.claude/wt"),
            "-Users-x-agent-dash--claude-wt"
        );
        assert_eq!(encode_cwd("/a b/c_d"), "-a-b-c-d");
        assert_eq!(encode_cwd("/é"), "--");
    }

    #[test]
    fn test_last_timestamp_skips_trailing_metadata() {
        let dir = TempDir::new("meta");
        let path = write_lines(
            &dir,
            &[
                message("user", "2026-10-13T03:00:00.000Z"),
                message("assistant", "2026-10-13T03:29:04.159Z"),
                r#"{"type":"ai-title","timestamp":"2030-01-01T00:00:00.000Z"}"#.to_string(),
                r#"{"type":"last-prompt"}"#.to_string(),
                "not json {{{".to_string(),
            ],
        );
        assert_eq!(
            last_message_timestamp(&path, CHUNK_SIZE).unwrap(),
            Some(epoch("2026-10-13T03:29:04.159Z"))
        );
    }

    #[test]
    fn test_last_timestamp_across_chunk_boundary() {
        let dir = TempDir::new("chunk");
        let path = write_lines(
            &dir,
            &[
                message("user", "2026-10-13T03:00:00.000Z"),
                message("assistant", "2026-10-13T04:00:00.000Z"),
                r#"{"type":"permission-mode","padding":"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"}"#
                    .to_string(),
                r#"{"type":"attachment","padding":"yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy"}"#
                    .to_string(),
            ],
        );
        // Every size, including ones that split the target line, must find the same answer.
        for chunk_size in 1..=path.metadata().unwrap().len() as usize + 1 {
            assert_eq!(
                last_message_timestamp(&path, chunk_size).unwrap(),
                Some(epoch("2026-10-13T04:00:00.000Z")),
                "chunk_size {}",
                chunk_size
            );
        }
    }

    #[test]
    fn test_no_message_lines_returns_none() {
        let dir = TempDir::new("none");
        let path = write_lines(&dir, &[r#"{"type":"ai-title"}"#.to_string()]);
        assert_eq!(last_message_timestamp(&path, CHUNK_SIZE).unwrap(), None);
    }

    #[test]
    fn test_resolve_path_prefers_encoded_cwd_then_scans() {
        let dir = TempDir::new("resolve");
        let encoded = dir.0.join("-tmp-proj");
        let other = dir.0.join("hashed-dir");
        std::fs::create_dir_all(&encoded).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(encoded.join("s1.jsonl"), "").unwrap();
        std::fs::write(other.join("s2.jsonl"), "").unwrap();

        assert_eq!(
            resolve_path(&dir.0, "s1", "/tmp/proj"),
            Some(encoded.join("s1.jsonl"))
        );
        assert_eq!(
            resolve_path(&dir.0, "s2", "/tmp/proj"),
            Some(other.join("s2.jsonl"))
        );
        assert_eq!(resolve_path(&dir.0, "missing", "/tmp/proj"), None);
    }
}
