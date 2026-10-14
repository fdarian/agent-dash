use memchr::memmem;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

const CHUNK_SIZE: usize = 64 * 1024;

#[derive(Deserialize)]
struct TranscriptLine {
    #[serde(rename = "type")]
    kind: String,
    timestamp: Option<String>,
    #[serde(rename = "lastPrompt")]
    last_prompt: Option<String>,
    #[serde(rename = "customTitle")]
    custom_title: Option<String>,
    #[serde(rename = "aiTitle")]
    ai_title: Option<String>,
    cwd: Option<String>,
}

struct Entry {
    path: PathBuf,
    len: u64,
    modified: SystemTime,
    last_activity: Option<u64>,
    /// The (len, mtime) `title` was read at, which can lag `len`/`modified`: the title is only
    /// read for sessions that ask for it.
    title_stamp: Option<(u64, SystemTime)>,
    title: Option<String>,
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
        let (entry, stat) = self.current(session_id, cwd)?;
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

    /// The `custom-title`, else `ai-title`, near the end of the transcript. `None` means there is
    /// none or no readable transcript.
    pub fn title(&mut self, session_id: &str, cwd: &str) -> Option<String> {
        let (entry, stat) = self.current(session_id, cwd)?;
        if entry.title_stamp != Some(stat) {
            let Ok(title) = title_in_tail(&entry.path, CHUNK_SIZE) else {
                self.entries.remove(session_id);
                return None;
            };
            entry.title_stamp = Some(stat);
            entry.title = title;
        }
        entry.title.clone()
    }

    fn current(&mut self, session_id: &str, cwd: &str) -> Option<(&mut Entry, (u64, SystemTime))> {
        let stat = self.stat(session_id, cwd)?;
        Some((self.entries.get_mut(session_id)?, stat))
    }

    /// The transcript's current (len, mtime), resolving its path on first use. A transcript that
    /// vanished is dropped so the next call resolves it again.
    fn stat(&mut self, session_id: &str, cwd: &str) -> Option<(u64, SystemTime)> {
        if !self.entries.contains_key(session_id) {
            let path = resolve_path(&projects_dir(), session_id, Some(cwd))?;
            self.entries.insert(
                session_id.to_string(),
                Entry {
                    path,
                    len: 0,
                    modified: SystemTime::UNIX_EPOCH,
                    last_activity: None,
                    title_stamp: None,
                    title: None,
                },
            );
        }
        let path = &self.entries.get(session_id)?.path;
        let stat = std::fs::metadata(path).and_then(|metadata| {
            metadata
                .modified()
                .map(|modified| (metadata.len(), modified))
        });
        if stat.is_err() {
            self.entries.remove(session_id);
        }
        stat.ok()
    }

    pub fn retain_sessions(&mut self, live_session_ids: &HashSet<&str>) {
        self.entries
            .retain(|session_id, _| live_session_ids.contains(session_id.as_str()));
    }
}

pub fn projects_dir() -> PathBuf {
    dirs::home_dir()
        .expect("home directory not found")
        .join(".claude/projects")
}

/// Whether any `~/.claude/projects/*/<session_id>.jsonl` exists.
pub fn transcript_exists(session_id: &str) -> bool {
    match scan_projects(&projects_dir(), session_id) {
        Ok(path) => path.is_some(),
        // Unknown beats a wrong "no": callers delete the session record when this is false.
        Err(_) => true,
    }
}

fn encode_cwd(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Claude names the project directory after the encoded cwd, but the cwd may have changed since
/// the session started and long paths are hashed, so fall back to scanning every project.
pub fn resolve_path(projects_dir: &Path, session_id: &str, cwd: Option<&str>) -> Option<PathBuf> {
    if let Some(cwd) = cwd {
        let expected = projects_dir
            .join(encode_cwd(cwd))
            .join(format!("{session_id}.jsonl"));
        if expected.is_file() {
            return Some(expected);
        }
    }
    scan_projects(projects_dir, session_id).ok().flatten()
}

/// A missing projects directory means no transcripts; any other read error is returned.
fn scan_projects(projects_dir: &Path, session_id: &str) -> io::Result<Option<PathBuf>> {
    let file_name = format!("{session_id}.jsonl");
    let projects = match std::fs::read_dir(projects_dir) {
        Ok(projects) => projects,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    Ok(projects
        .filter_map(|dir| dir.ok())
        .map(|dir| dir.path().join(&file_name))
        .find(|candidate| candidate.is_file()))
}

fn activity_time(parsed: &TranscriptLine) -> Option<u64> {
    if parsed.kind != "user" && parsed.kind != "assistant" {
        return None;
    }
    let timestamp = chrono::DateTime::parse_from_rfc3339(parsed.timestamp.as_deref()?).ok()?;
    u64::try_from(timestamp.timestamp()).ok()
}

fn parse_line(line: &[u8]) -> Option<u64> {
    activity_time(&serde_json::from_slice::<TranscriptLine>(line).ok()?)
}

/// Walks the file backwards so only the tail is read; transcripts can be many MB and end in
/// metadata lines (`ai-title`, `last-prompt`, ...) that carry no activity time. Calls `visit`
/// on each complete line from the end of the file, looking at no more than `max_bytes` from the
/// end, until it returns `ControlFlow::Break`.
fn for_each_line_rev(
    path: &Path,
    chunk_size: usize,
    max_bytes: u64,
    mut visit: impl FnMut(&[u8]) -> ControlFlow<()>,
) -> io::Result<()> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let lower_bound = len.saturating_sub(max_bytes);
    let mut position = len;
    // Everything after the chunk being read. Its first line may be cut off by the chunk
    // boundary, so it is only visited once the preceding chunk supplies the rest.
    let mut carry: Vec<u8> = Vec::new();
    while position > lower_bound {
        let read_len = (position - lower_bound).min(chunk_size as u64);
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
        for line in buf[complete_start..].split(|&b| b == b'\n').rev() {
            if visit(line).is_break() {
                return Ok(());
            }
        }
        buf.truncate(complete_start);
        carry = buf;
    }
    Ok(())
}

/// The first (i.e. last in the file) line `parse` accepts, looking at no more than `max_bytes`
/// from the end.
fn find_last_line<T>(
    path: &Path,
    chunk_size: usize,
    max_bytes: u64,
    parse: impl Fn(&[u8]) -> Option<T>,
) -> io::Result<Option<T>> {
    let mut found = None;
    for_each_line_rev(path, chunk_size, max_bytes, |line| {
        found = parse(line);
        if found.is_some() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    })?;
    Ok(found)
}

fn last_message_timestamp(path: &Path, chunk_size: usize) -> io::Result<Option<u64>> {
    find_last_line(path, chunk_size, u64::MAX, parse_line)
}

/// What an untracked transcript says about its session.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TranscriptSummary {
    /// `custom-title`, then `ai-title`, then the first user prompt, then the last prompt.
    pub title: Option<String>,
    /// From the last record that carries one.
    pub cwd: Option<String>,
    pub last_activity: Option<u64>,
}

#[derive(Default)]
struct SummaryAccumulator {
    custom_title: Option<String>,
    ai_title: Option<String>,
    last_prompt: Option<String>,
    cwd: Option<String>,
    last_activity: Option<u64>,
}

/// Substrings of the records worth parsing once the activity time and cwd are known. Nearly
/// every other line is a multi-KB message or attachment that would be parsed for nothing.
struct TitleNeedles {
    finders: [memmem::Finder<'static>; 3],
}

impl TitleNeedles {
    fn new() -> TitleNeedles {
        TitleNeedles {
            finders: [
                memmem::Finder::new("custom-title"),
                memmem::Finder::new("ai-title"),
                memmem::Finder::new("last-prompt"),
            ],
        }
    }

    fn in_line(&self, line: &[u8]) -> bool {
        self.finders
            .iter()
            .any(|finder| finder.find(line).is_some())
    }
}

impl SummaryAccumulator {
    /// Keeps the first value seen per field; lines arrive last-first.
    fn visit(&mut self, line: &[u8], needles: &TitleNeedles) {
        let needs_message = self.last_activity.is_none() || self.cwd.is_none();
        if !needs_message && !needles.in_line(line) {
            return;
        }
        let Ok(parsed) = serde_json::from_slice::<TranscriptLine>(line) else {
            return;
        };
        if self.last_activity.is_none() {
            self.last_activity = activity_time(&parsed);
        }
        let non_blank = |text: Option<String>| text.filter(|text| !text.trim().is_empty());
        if self.custom_title.is_none() && parsed.kind == "custom-title" {
            self.custom_title = non_blank(parsed.custom_title);
        }
        if self.ai_title.is_none() && parsed.kind == "ai-title" {
            self.ai_title = non_blank(parsed.ai_title);
        }
        if self.last_prompt.is_none() && parsed.kind == "last-prompt" {
            self.last_prompt = non_blank(parsed.last_prompt);
        }
        if self.cwd.is_none() {
            self.cwd = non_blank(parsed.cwd);
        }
    }

    fn has_title(&self) -> bool {
        self.custom_title.is_some() || self.ai_title.is_some()
    }

    /// A `custom-title` outranks everything else, so only that, the cwd and the activity time
    /// are worth stopping early for.
    fn is_complete(&self) -> bool {
        self.custom_title.is_some() && self.cwd.is_some() && self.last_activity.is_some()
    }

    fn finish(self, first_prompt: Option<String>) -> TranscriptSummary {
        TranscriptSummary {
            title: self
                .custom_title
                .or(self.ai_title)
                .or(first_prompt)
                .or_else(|| self.last_prompt.map(|prompt| clean_prompt(&prompt))),
            cwd: self.cwd,
            last_activity: self.last_activity,
        }
    }
}

fn scan_summary(path: &Path, chunk_size: usize, max_bytes: u64) -> io::Result<SummaryAccumulator> {
    let needles = TitleNeedles::new();
    let mut summary = SummaryAccumulator::default();
    for_each_line_rev(path, chunk_size, max_bytes, |line| {
        summary.visit(line, &needles);
        if summary.is_complete() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    })?;
    Ok(summary)
}

/// A `custom-title` outranks any `ai-title`, so the scan runs to the first one or the end of the
/// tail.
fn title_in_tail(path: &Path, chunk_size: usize) -> io::Result<Option<String>> {
    let custom = memmem::Finder::new("custom-title");
    let ai = memmem::Finder::new("ai-title");
    let mut custom_title = None;
    let mut ai_title = None;
    for_each_line_rev(path, chunk_size, SUMMARY_TAIL_MAX_BYTES, |line| {
        if custom.find(line).is_some() || ai.find(line).is_some() {
            if let Ok(parsed) = serde_json::from_slice::<TranscriptLine>(line) {
                let non_blank = |text: Option<String>| text.filter(|text| !text.trim().is_empty());
                if custom_title.is_none() && parsed.kind == "custom-title" {
                    custom_title = non_blank(parsed.custom_title);
                }
                if ai_title.is_none() && parsed.kind == "ai-title" {
                    ai_title = non_blank(parsed.ai_title);
                }
            }
        }
        if custom_title.is_some() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    })?;
    Ok(custom_title.or(ai_title))
}

/// How much of the start of a transcript to read looking for the first user prompt.
const HEAD_MAX_BYTES: u64 = 64 * 1024;

/// Longest title taken from a prompt, in characters.
const PROMPT_TITLE_CHARS: usize = 100;

#[derive(Deserialize)]
struct HeadLine {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "isMeta", default)]
    is_meta: bool,
    #[serde(rename = "isSidechain", default)]
    is_sidechain: bool,
    message: Option<HeadMessage>,
}

#[derive(Deserialize)]
struct HeadMessage {
    content: Option<HeadContent>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum HeadContent {
    Text(String),
    Blocks(Vec<HeadBlock>),
}

#[derive(Deserialize)]
struct HeadBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

fn clean_prompt(prompt: &str) -> String {
    let collapsed = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(PROMPT_TITLE_CHARS).collect()
}

/// The text of a prompt the user typed. Skips tool results, meta records and the `<command-...>`
/// / `<system-reminder>` wrappers Claude stores as user messages.
fn typed_prompt(line: &[u8]) -> Option<String> {
    let parsed = serde_json::from_slice::<HeadLine>(line).ok()?;
    if parsed.kind != "user" || parsed.is_meta || parsed.is_sidechain {
        return None;
    }
    let text = match parsed.message?.content? {
        HeadContent::Text(text) => text,
        HeadContent::Blocks(blocks) => {
            blocks
                .into_iter()
                .find(|block| block.kind == "text")?
                .text?
        }
    };
    let text = text.trim_start();
    if text.is_empty() || text.starts_with('<') {
        return None;
    }
    Some(clean_prompt(text))
}

/// Reads at most `max_bytes` from the start, never the whole transcript.
fn first_user_prompt(path: &Path, max_bytes: u64) -> io::Result<Option<String>> {
    let mut head = Vec::new();
    File::open(path)?.take(max_bytes).read_to_end(&mut head)?;
    let mut lines = head.split(|&b| b == b'\n');
    // The last segment is cut off by the limit unless the file ended inside it.
    let truncated = head.len() as u64 == max_bytes;
    if truncated {
        lines.next_back();
    }
    let needle = memmem::Finder::new("\"user\"");
    Ok(lines
        .filter(|line| needle.find(line).is_some())
        .find_map(typed_prompt))
}

fn summarize_with(path: &Path, chunk_size: usize) -> io::Result<TranscriptSummary> {
    let tail = scan_summary(path, chunk_size, SUMMARY_TAIL_MAX_BYTES)?;
    // Titles are re-appended near the tail, so a miss there is rare, and the answer then comes
    // from the start of the file rather than from reading all of it.
    let first_prompt = if tail.has_title() {
        None
    } else {
        first_user_prompt(path, HEAD_MAX_BYTES)?
    };
    Ok(tail.finish(first_prompt))
}

/// Title, cwd and last activity of a transcript, read from the tail and, when it names no
/// session, the head.
pub fn summarize(path: &Path) -> io::Result<TranscriptSummary> {
    summarize_with(path, CHUNK_SIZE)
}

/// How far from the end of a transcript to look for titles.
const SUMMARY_TAIL_MAX_BYTES: u64 = 1024 * 1024;

/// How far from the end of a transcript to look for the `last-prompt` record. Claude re-appends
/// it near the tail, so a miss this deep means the transcript has none worth a long scan.
const LAST_PROMPT_MAX_BYTES: u64 = 1024 * 1024;

fn parse_last_prompt(line: &[u8]) -> Option<String> {
    let parsed = serde_json::from_slice::<TranscriptLine>(line).ok()?;
    if parsed.kind != "last-prompt" {
        return None;
    }
    parsed
        .last_prompt
        .filter(|prompt| !prompt.trim().is_empty())
}

fn last_prompt_in_tail(path: &Path, chunk_size: usize) -> io::Result<Option<String>> {
    find_last_line(path, chunk_size, LAST_PROMPT_MAX_BYTES, parse_last_prompt)
}

struct PromptEntry {
    path: PathBuf,
    len: u64,
    modified: SystemTime,
    last_prompt: Option<String>,
}

/// How long a missing transcript is trusted before the projects directory is scanned again.
const MISS_RECHECK: Duration = Duration::from_secs(30);

/// A lookup that found no transcript.
struct Miss {
    checked_at: Instant,
    /// mtime of the project directory the transcript was expected in; a new file there changes
    /// it. `None` when the directory doesn't exist.
    project_modified: Option<SystemTime>,
}

/// The `last-prompt` text of sessions that aren't running, for the closed-session preview.
/// Like `TranscriptCache`, a transcript is only re-read when its length or mtime changes, so
/// this is safe to call on every selection change. A session without a transcript is looked up
/// again only when its project directory changes or after `MISS_RECHECK`, since each lookup
/// scans every project directory.
#[derive(Default)]
pub struct LastPromptCache {
    entries: HashMap<String, PromptEntry>,
    misses: HashMap<String, Miss>,
}

impl LastPromptCache {
    pub fn get(&mut self, session_id: &str, cwd: &str) -> Option<String> {
        self.get_in(&projects_dir(), session_id, cwd, Instant::now())
    }

    fn get_in(
        &mut self,
        projects: &Path,
        session_id: &str,
        cwd: &str,
        now: Instant,
    ) -> Option<String> {
        if !self.entries.contains_key(session_id) {
            let project_modified = std::fs::metadata(projects.join(encode_cwd(cwd)))
                .and_then(|metadata| metadata.modified())
                .ok();
            if let Some(miss) = self.misses.get(session_id) {
                if miss.project_modified == project_modified
                    && now.duration_since(miss.checked_at) < MISS_RECHECK
                {
                    return None;
                }
            }
            let Some(path) = resolve_path(projects, session_id, Some(cwd)) else {
                self.misses.insert(
                    session_id.to_string(),
                    Miss {
                        checked_at: now,
                        project_modified,
                    },
                );
                return None;
            };
            self.misses.remove(session_id);
            self.entries.insert(
                session_id.to_string(),
                PromptEntry {
                    path,
                    len: 0,
                    modified: SystemTime::UNIX_EPOCH,
                    last_prompt: None,
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
            self.entries.remove(session_id);
            return None;
        };
        if stat != (entry.len, entry.modified) {
            let Ok(last_prompt) = last_prompt_in_tail(&entry.path, CHUNK_SIZE) else {
                self.entries.remove(session_id);
                return None;
            };
            entry.len = stat.0;
            entry.modified = stat.1;
            entry.last_prompt = last_prompt;
        }
        entry.last_prompt.clone()
    }
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
    fn test_last_prompt_takes_the_latest_record() {
        let dir = TempDir::new("prompt");
        let path = write_lines(
            &dir,
            &[
                r#"{"type":"last-prompt","lastPrompt":"old one"}"#.to_string(),
                message("user", "2026-10-13T03:00:00.000Z"),
                r#"{"type":"last-prompt","lastPrompt":"fix the \"bug\"\nplease"}"#.to_string(),
                r#"{"type":"ai-title","aiTitle":"t"}"#.to_string(),
                "not json {{{".to_string(),
            ],
        );
        assert_eq!(
            last_prompt_in_tail(&path, CHUNK_SIZE).unwrap().as_deref(),
            Some("fix the \"bug\"\nplease")
        );
        for chunk_size in 1..=path.metadata().unwrap().len() as usize + 1 {
            assert_eq!(
                last_prompt_in_tail(&path, chunk_size).unwrap().as_deref(),
                Some("fix the \"bug\"\nplease"),
                "chunk_size {}",
                chunk_size
            );
        }
    }

    #[test]
    fn test_last_prompt_missing_or_blank_is_none() {
        let dir = TempDir::new("prompt-none");
        let path = write_lines(
            &dir,
            &[
                message("user", "2026-10-13T03:00:00.000Z"),
                r#"{"type":"last-prompt","lastPrompt":"  "}"#.to_string(),
                r#"{"type":"last-prompt"}"#.to_string(),
            ],
        );
        assert_eq!(last_prompt_in_tail(&path, CHUNK_SIZE).unwrap(), None);
    }

    #[test]
    fn test_last_prompt_beyond_the_tail_limit_is_not_found() {
        let dir = TempDir::new("prompt-far");
        let filler = format!(
            r#"{{"type":"attachment","padding":"{}"}}"#,
            "x".repeat(1000)
        );
        let mut lines = vec![r#"{"type":"last-prompt","lastPrompt":"far"}"#.to_string()];
        lines.extend(std::iter::repeat_n(filler, 1100));
        let path = write_lines(&dir, &lines);
        assert_eq!(last_prompt_in_tail(&path, CHUNK_SIZE).unwrap(), None);
        assert_eq!(
            find_last_line(&path, CHUNK_SIZE, u64::MAX, parse_last_prompt)
                .unwrap()
                .as_deref(),
            Some("far")
        );
    }

    #[test]
    fn test_summary_prefers_custom_title_and_reads_last_cwd_at_any_chunk_size() {
        let dir = TempDir::new("summary");
        let path = write_lines(
            &dir,
            &[
                r#"{"type":"user","timestamp":"2026-10-13T01:00:00.000Z","cwd":"/old"}"#
                    .to_string(),
                r#"{"type":"custom-title","customTitle":"Named"}"#.to_string(),
                r#"{"type":"assistant","timestamp":"2026-10-13T02:00:00.000Z","cwd":"/new"}"#
                    .to_string(),
                r#"{"type":"ai-title","aiTitle":"Generated"}"#.to_string(),
                r#"{"type":"last-prompt","lastPrompt":"p"}"#.to_string(),
            ],
        );
        let expected = TranscriptSummary {
            title: Some("Named".to_string()),
            cwd: Some("/new".to_string()),
            last_activity: Some(epoch("2026-10-13T02:00:00.000Z")),
        };
        for chunk_size in 1..=path.metadata().unwrap().len() as usize + 1 {
            assert_eq!(
                summarize_with(&path, chunk_size).unwrap(),
                expected,
                "chunk_size {}",
                chunk_size
            );
        }
    }

    #[test]
    fn title_in_tail_prefers_custom_over_a_later_ai_title_at_any_chunk_size() {
        let dir = TempDir::new("title-tail");
        let path = write_lines(
            &dir,
            &[
                r#"{"type":"ai-title","aiTitle":"Older AI"}"#.to_string(),
                r#"{"type":"custom-title","customTitle":"Named"}"#.to_string(),
                message("user", "2026-10-13T01:00:00.000Z"),
                r#"{"type":"ai-title","aiTitle":"Newer AI"}"#.to_string(),
                r#"{"type":"last-prompt","lastPrompt":"p"}"#.to_string(),
            ],
        );
        for chunk_size in 1..=path.metadata().unwrap().len() as usize + 1 {
            assert_eq!(
                title_in_tail(&path, chunk_size).unwrap().as_deref(),
                Some("Named"),
                "chunk_size {chunk_size}"
            );
        }
    }

    #[test]
    fn title_in_tail_falls_back_to_the_newest_ai_title_and_none_without_either() {
        let dir = TempDir::new("title-tail-ai");
        let path = write_lines(
            &dir,
            &[
                r#"{"type":"ai-title","aiTitle":"Older AI"}"#.to_string(),
                r#"{"type":"ai-title","aiTitle":"Newer AI"}"#.to_string(),
            ],
        );
        assert_eq!(
            title_in_tail(&path, CHUNK_SIZE).unwrap().as_deref(),
            Some("Newer AI")
        );
        let path = write_lines(&dir, &[message("user", "2026-10-13T01:00:00.000Z")]);
        assert_eq!(title_in_tail(&path, CHUNK_SIZE).unwrap(), None);
    }

    #[test]
    fn first_user_prompt_skips_meta_tool_results_and_wrappers() {
        let dir = TempDir::new("head-prompt");
        let path = write_lines(
            &dir,
            &[
                r#"{"type":"mode"}"#.to_string(),
                r#"{"type":"user","isMeta":true,"message":{"content":"meta text"}}"#.to_string(),
                r#"{"type":"user","message":{"content":"<system-reminder>x</system-reminder>"}}"#
                    .to_string(),
                r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"out"}]}}"#
                    .to_string(),
                r#"{"type":"user","isSidechain":true,"message":{"content":"sub"}}"#.to_string(),
                r#"{"type":"user","message":{"content":[{"type":"image"},{"type":"text","text":"real  ask"}]}}"#
                    .to_string(),
                r#"{"type":"user","message":{"content":"second"}}"#.to_string(),
            ],
        );
        assert_eq!(
            first_user_prompt(&path, HEAD_MAX_BYTES).unwrap().as_deref(),
            Some("real ask")
        );
    }

    #[test]
    fn first_user_prompt_is_cut_to_100_chars_and_never_reads_past_the_limit() {
        let dir = TempDir::new("head-limit");
        let long = "w".repeat(500);
        let path = write_lines(
            &dir,
            &[
                format!(
                    r#"{{"type":"attachment","padding":"{}"}}"#,
                    "p".repeat(2000)
                ),
                format!(r#"{{"type":"user","message":{{"content":"{long}"}}}}"#),
            ],
        );
        assert_eq!(first_user_prompt(&path, 1000).unwrap(), None);
        assert_eq!(
            first_user_prompt(&path, HEAD_MAX_BYTES).unwrap(),
            Some("w".repeat(100))
        );
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
            resolve_path(&dir.0, "s1", Some("/tmp/proj")),
            Some(encoded.join("s1.jsonl"))
        );
        assert_eq!(
            resolve_path(&dir.0, "s2", Some("/tmp/proj")),
            Some(other.join("s2.jsonl"))
        );
        assert_eq!(resolve_path(&dir.0, "missing", Some("/tmp/proj")), None);
    }

    fn prompt_line(prompt: &str) -> String {
        format!(r#"{{"type":"last-prompt","lastPrompt":"{prompt}"}}"#)
    }

    #[test]
    fn last_prompt_cache_remembers_a_missing_transcript_for_30s() {
        let dir = TempDir::new("prompt-miss-ttl");
        let start = Instant::now();
        let mut cache = LastPromptCache::default();
        assert_eq!(cache.get_in(&dir.0, "s1", "/tmp/proj", start), None);

        // Lands in a directory other than the expected one, so only the timeout can find it.
        let other = dir.0.join("hashed-dir");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("s1.jsonl"), prompt_line("hi") + "\n").unwrap();

        let soon = start + Duration::from_secs(29);
        assert_eq!(cache.get_in(&dir.0, "s1", "/tmp/proj", soon), None);
        let later = start + Duration::from_secs(31);
        assert_eq!(
            cache.get_in(&dir.0, "s1", "/tmp/proj", later).as_deref(),
            Some("hi")
        );
    }

    #[test]
    fn last_prompt_cache_rechecks_a_miss_when_the_project_dir_changes() {
        let dir = TempDir::new("prompt-miss-mtime");
        let project = dir.0.join("-tmp-proj");
        std::fs::create_dir_all(&project).unwrap();
        // Pinned so the file created below is guaranteed to move it, whatever the fs resolution.
        std::fs::File::open(&project)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000))
            .unwrap();

        let start = Instant::now();
        let mut cache = LastPromptCache::default();
        assert_eq!(cache.get_in(&dir.0, "s1", "/tmp/proj", start), None);

        std::fs::write(project.join("s1.jsonl"), prompt_line("hi") + "\n").unwrap();
        assert_eq!(
            cache
                .get_in(&dir.0, "s1", "/tmp/proj", start + Duration::from_secs(1))
                .as_deref(),
            Some("hi")
        );
    }
}
