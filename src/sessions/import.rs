use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::is_session_uuid;
use super::record::Origin;
use crate::claude_transcript;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportSource {
    Cli,
    Desktop,
}

impl ImportSource {
    pub fn label(self) -> &'static str {
        match self {
            ImportSource::Cli => "CLI",
            ImportSource::Desktop => "Desktop",
        }
    }

    pub fn origin(self) -> Origin {
        match self {
            ImportSource::Cli => Origin::ImportedCli,
            ImportSource::Desktop => Origin::ImportedDesktop,
        }
    }
}

/// A session found on disk that agent-dash isn't tracking yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportCandidate {
    pub session_id: String,
    pub source: ImportSource,
    /// Empty when the session was never named.
    pub title: String,
    pub cwd: String,
    pub last_activity: u64,
}

#[derive(Debug, Default)]
pub struct ScanOutcome {
    /// Newest activity first.
    pub candidates: Vec<ImportCandidate>,
    /// Files that couldn't be read or didn't say where the session ran.
    pub skipped: usize,
}

/// Where Claude keeps the sessions to import from.
pub struct ImportRoots {
    /// `~/.claude/projects`
    pub projects: PathBuf,
    /// `~/Library/Application Support/Claude/claude-code-sessions`
    pub desktop: PathBuf,
}

impl ImportRoots {
    pub fn from_home() -> ImportRoots {
        let home = dirs::home_dir().expect("home directory not found");
        ImportRoots {
            projects: claude_transcript::projects_dir(),
            desktop: home.join("Library/Application Support/Claude/claude-code-sessions"),
        }
    }
}

/// Lists CLI and Desktop sessions that aren't in `known`. When both know a session, Desktop's
/// metadata wins. A root that doesn't exist counts as having no sessions.
pub fn scan(roots: &ImportRoots, known: &HashSet<String>) -> Result<ScanOutcome> {
    let mut skipped = 0;
    let mut found: HashMap<String, ImportCandidate> = HashMap::new();

    for candidate in scan_cli(&roots.projects, &mut skipped)? {
        found.insert(candidate.session_id.clone(), candidate);
    }
    let desktop = scan_desktop(&roots.desktop, &roots.projects, &mut skipped)?;
    // Archiving in Desktop hides the session everywhere, including its CLI transcript.
    for session_id in &desktop.archived {
        found.remove(session_id);
    }
    for candidate in desktop.offered {
        if desktop.archived.contains(&candidate.session_id) {
            continue;
        }
        let older = found.get(&candidate.session_id).is_some_and(|existing| {
            existing.source == ImportSource::Desktop
                && existing.last_activity > candidate.last_activity
        });
        if !older {
            found.insert(candidate.session_id.clone(), candidate);
        }
    }

    let mut candidates: Vec<ImportCandidate> = found
        .into_values()
        .filter(|candidate| !known.contains(&candidate.session_id))
        .collect();
    candidates.sort_by(|a, b| {
        b.last_activity
            .cmp(&a.last_activity)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });
    Ok(ScanOutcome {
        candidates,
        skipped,
    })
}

fn read_dir_if_present(dir: &Path) -> Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err).with_context(|| format!("failed to read {}", dir.display())),
    };
    let mut paths = Vec::new();
    for entry in entries {
        paths.push(
            entry
                .with_context(|| format!("failed to read {}", dir.display()))?
                .path(),
        );
    }
    paths.sort();
    Ok(paths)
}

/// Runs `work` on every item across threads, keeping the results in item order. Scanning is
/// dominated by file IO and JSON parsing that don't depend on each other.
fn parallel_map<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    const CHUNK: usize = 16;
    let chunks: Vec<&[T]> = items.chunks(CHUNK).collect();
    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(chunks.len());
    let mut done: Vec<(usize, Vec<R>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut finished = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(chunk) = chunks.get(index) else {
                            return finished;
                        };
                        finished.push((index, chunk.iter().map(&work).collect::<Vec<R>>()));
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("scan worker panicked"))
            .collect()
    });
    done.sort_by_key(|(index, _)| *index);
    done.into_iter().flat_map(|(_, results)| results).collect()
}

struct CliTranscript {
    path: PathBuf,
    session_id: String,
}

fn cli_transcripts(projects: &Path, skipped: &mut usize) -> Result<Vec<CliTranscript>> {
    let mut found = Vec::new();
    for project in read_dir_if_present(projects)? {
        // Subagent transcripts live in `<project>/<session id>/subagents/`, one level too deep
        // to match below; the name check also covers a flat `subagents` directory.
        if !project.is_dir() || project.file_name().is_some_and(|name| name == "subagents") {
            continue;
        }
        let Ok(files) = read_dir_if_present(&project) else {
            *skipped += 1;
            continue;
        };
        for path in files {
            if path.extension().is_none_or(|ext| ext != "jsonl") {
                continue;
            }
            let session_id = path
                .file_stem()
                .expect("a path with an extension has a stem")
                .to_string_lossy()
                .into_owned();
            // Not a session transcript, e.g. an `agent-*.jsonl` sidechain.
            if !is_session_uuid(&session_id) || !path.is_file() {
                continue;
            }
            found.push(CliTranscript { path, session_id });
        }
    }
    Ok(found)
}

fn scan_cli(projects: &Path, skipped: &mut usize) -> Result<Vec<ImportCandidate>> {
    let transcripts = cli_transcripts(projects, skipped)?;
    let mut candidates = Vec::new();
    for result in parallel_map(&transcripts, |transcript| {
        cli_candidate(&transcript.path, transcript.session_id.clone())
    }) {
        match result {
            Ok(candidate) => candidates.push(candidate),
            Err(_) => *skipped += 1,
        }
    }
    Ok(candidates)
}

fn cli_candidate(path: &Path, session_id: String) -> Result<ImportCandidate> {
    let summary = claude_transcript::summarize(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let last_activity = match summary.last_activity {
        Some(last_activity) => last_activity,
        None => mtime_secs(path)?,
    };
    let cwd = summary
        .cwd
        .ok_or_else(|| anyhow!("{} doesn't say where the session ran", path.display()))?;
    Ok(ImportCandidate {
        session_id,
        source: ImportSource::Cli,
        title: summary.title.unwrap_or_default(),
        cwd,
        last_activity,
    })
}

fn mtime_secs(path: &Path) -> Result<u64> {
    let modified = std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .with_context(|| format!("failed to read the mtime of {}", path.display()))?;
    let elapsed = modified
        .duration_since(std::time::UNIX_EPOCH)
        .with_context(|| format!("{} was modified before the unix epoch", path.display()))?;
    Ok(elapsed.as_secs())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopSession {
    cli_session_id: Option<String>,
    cwd: Option<String>,
    title: Option<String>,
    /// Milliseconds since the epoch.
    last_activity_at: Option<u64>,
    #[serde(default)]
    is_archived: bool,
}

#[derive(Default)]
struct DesktopScan {
    offered: Vec<ImportCandidate>,
    /// CLI session ids of archived Desktop entries.
    archived: HashSet<String>,
}

/// What one Desktop file means for the import list.
enum DesktopEntry {
    Offer(ImportCandidate),
    Archived(String),
    /// No CLI session, or its transcript is gone.
    Ignore,
}

/// `<root>/<account>/<org>/local_*.json`. `deleted_*` stubs don't match the file name.
fn desktop_files(root: &Path, skipped: &mut usize) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for account in read_dir_if_present(root)? {
        if !account.is_dir() {
            continue;
        }
        let Ok(orgs) = read_dir_if_present(&account) else {
            *skipped += 1;
            continue;
        };
        for org in orgs.into_iter().filter(|org| org.is_dir()) {
            let Ok(files) = read_dir_if_present(&org) else {
                *skipped += 1;
                continue;
            };
            found.extend(files.into_iter().filter(|path| {
                path.extension().is_some_and(|ext| ext == "json")
                    && path
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with("local_"))
            }));
        }
    }
    Ok(found)
}

fn scan_desktop(root: &Path, projects: &Path, skipped: &mut usize) -> Result<DesktopScan> {
    let files = desktop_files(root, skipped)?;
    let mut found = DesktopScan::default();
    for result in parallel_map(&files, |path| desktop_candidate(path, projects)) {
        match result {
            Ok(DesktopEntry::Offer(candidate)) => found.offered.push(candidate),
            Ok(DesktopEntry::Archived(session_id)) => {
                found.archived.insert(session_id);
            }
            Ok(DesktopEntry::Ignore) => {}
            Err(_) => *skipped += 1,
        }
    }
    Ok(found)
}

/// `Err` for files that couldn't be understood.
fn desktop_candidate(path: &Path, projects: &Path) -> Result<DesktopEntry> {
    let content =
        std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let session: DesktopSession = serde_json::from_slice(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    let Some(session_id) = session.cli_session_id.filter(|id| !id.is_empty()) else {
        return Ok(DesktopEntry::Ignore);
    };
    if !is_session_uuid(&session_id) {
        bail!("{} has a cliSessionId that isn't a UUID", path.display());
    }
    if session.is_archived {
        return Ok(DesktopEntry::Archived(session_id));
    }
    let desktop_cwd = session.cwd.filter(|cwd| !cwd.is_empty());
    let Some(transcript) =
        claude_transcript::resolve_path(projects, &session_id, desktop_cwd.as_deref())
    else {
        return Ok(DesktopEntry::Ignore);
    };

    let desktop_title = session.title.filter(|title| !title.trim().is_empty());
    let (title, cwd) = match (desktop_title, desktop_cwd) {
        (Some(title), Some(cwd)) => (title, cwd),
        (title, cwd) => {
            let summary = claude_transcript::summarize(&transcript)
                .with_context(|| format!("failed to read {}", transcript.display()))?;
            let cwd = cwd.or(summary.cwd).ok_or_else(|| {
                anyhow!("{} doesn't say where the session ran", transcript.display())
            })?;
            (title.or(summary.title).unwrap_or_default(), cwd)
        }
    };
    let last_activity = match session.last_activity_at {
        Some(millis) => millis / 1000,
        None => mtime_secs(&transcript)?,
    };
    Ok(DesktopEntry::Offer(ImportCandidate {
        session_id,
        source: ImportSource::Desktop,
        title,
        cwd,
        last_activity,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::record::tests::TempDir;

    struct Fixture {
        // Held for its Drop, which removes the fixture files.
        _dir: TempDir,
        roots: ImportRoots,
    }

    impl Fixture {
        fn new(name: &str) -> Fixture {
            let dir = TempDir::new(name);
            let roots = ImportRoots {
                projects: dir.0.join("projects"),
                desktop: dir.0.join("desktop"),
            };
            Fixture { _dir: dir, roots }
        }

        fn write(&self, path: PathBuf, content: &str) {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }

        fn transcript(&self, project: &str, session_id: &str, lines: &[&str]) {
            self.write(
                self.roots
                    .projects
                    .join(project)
                    .join(format!("{session_id}.jsonl")),
                &(lines.join("\n") + "\n"),
            );
        }

        fn desktop(&self, file: &str, json: &str) {
            self.write(self.roots.desktop.join("acct/org").join(file), json);
        }

        fn scan(&self) -> ScanOutcome {
            scan(&self.roots, &HashSet::new()).unwrap()
        }
    }

    const USER_AT_10: &str = r#"{"type":"user","timestamp":"2026-10-10T00:00:00.000Z","cwd":"/work/app","message":"hi"}"#;
    const USER_AT_10_EPOCH: u64 = 1_791_590_400;

    fn ids(outcome: &ScanOutcome) -> Vec<&str> {
        outcome
            .candidates
            .iter()
            .map(|c| c.session_id.as_str())
            .collect()
    }

    #[test]
    fn cli_session_takes_title_cwd_and_activity_from_the_transcript() {
        let fx = Fixture::new("import-cli");
        fx.transcript(
            "-work-app",
            "00000000-0000-4000-8000-000000000001",
            &[
                USER_AT_10,
                r#"{"type":"ai-title","aiTitle":"AI name"}"#,
                r#"{"type":"custom-title","customTitle":"My name"}"#,
                r#"{"type":"last-prompt","lastPrompt":"p"}"#,
            ],
        );

        let outcome = fx.scan();
        assert_eq!(
            outcome.candidates,
            vec![ImportCandidate {
                session_id: "00000000-0000-4000-8000-000000000001".to_string(),
                source: ImportSource::Cli,
                title: "My name".to_string(),
                cwd: "/work/app".to_string(),
                last_activity: USER_AT_10_EPOCH,
            }]
        );
        assert_eq!(outcome.skipped, 0);
    }

    #[test]
    fn cli_title_falls_back_to_ai_title() {
        let fx = Fixture::new("import-ai-title");
        fx.transcript(
            "-work-app",
            "00000000-0000-4000-8000-000000000001",
            &[USER_AT_10, r#"{"type":"ai-title","aiTitle":"AI name"}"#],
        );
        assert_eq!(fx.scan().candidates[0].title, "AI name");
    }

    fn deep_transcript(fx: &Fixture, first_lines: &[&str], last_lines: &[&str]) {
        let filler = format!(
            r#"{{"type":"attachment","cwd":"/work/app","padding":"{}"}}"#,
            "x".repeat(1000)
        );
        let mut lines = first_lines.to_vec();
        lines.extend(std::iter::repeat_n(filler.as_str(), 1100));
        lines.extend(last_lines);
        fx.transcript("-work-app", "00000000-0000-4000-8000-000000000001", &lines);
    }

    const FIRST_PROMPT: &str = r#"{"type":"user","timestamp":"2026-10-10T00:00:00.000Z","cwd":"/work/app","message":{"role":"user","content":"  fix the\nlogin bug "}}"#;

    #[test]
    fn title_beyond_the_tail_falls_back_to_the_first_user_prompt() {
        let fx = Fixture::new("import-deep-title");
        deep_transcript(
            &fx,
            &[
                r#"{"type":"custom-title","customTitle":"Deep"}"#,
                FIRST_PROMPT,
            ],
            &[r#"{"type":"last-prompt","lastPrompt":"later prompt"}"#],
        );
        assert_eq!(fx.scan().candidates[0].title, "fix the login bug");
    }

    #[test]
    fn without_a_first_prompt_in_the_head_the_last_prompt_is_the_title() {
        let fx = Fixture::new("import-last-prompt-title");
        deep_transcript(
            &fx,
            &[
                r#"{"type":"user","cwd":"/work/app","message":{"content":"<command-name>/x</command-name>"}}"#,
            ],
            &[r#"{"type":"last-prompt","lastPrompt":"later\nprompt"}"#],
        );
        assert_eq!(fx.scan().candidates[0].title, "later prompt");
    }

    #[test]
    fn cli_session_without_any_title_is_still_offered_untitled() {
        let fx = Fixture::new("import-untitled");
        fx.transcript(
            "-work-app",
            "abcdef01-2345-4789-8abc-def012345678",
            &[USER_AT_10],
        );
        let candidate = &fx.scan().candidates[0];
        assert_eq!(candidate.title, "");
        assert_eq!(
            crate::sessions::display_title(&candidate.title, &candidate.session_id),
            "abcdef01"
        );
    }

    #[test]
    fn cli_activity_falls_back_to_file_mtime_without_message_records() {
        let fx = Fixture::new("import-mtime");
        fx.transcript(
            "-work-app",
            "00000000-0000-4000-8000-000000000001",
            &[r#"{"type":"custom-title","customTitle":"T","cwd":"/work/app"}"#],
        );
        assert!(fx.scan().candidates[0].last_activity > USER_AT_10_EPOCH);
    }

    #[test]
    fn transcript_without_a_cwd_is_skipped_and_counted() {
        let fx = Fixture::new("import-no-cwd");
        fx.transcript(
            "-work-app",
            "00000000-0000-4000-8000-000000000001",
            &[r#"{"type":"custom-title","customTitle":"T"}"#],
        );
        let outcome = fx.scan();
        assert!(outcome.candidates.is_empty());
        assert_eq!(outcome.skipped, 1);
    }

    #[test]
    fn subagent_transcripts_are_skipped() {
        let fx = Fixture::new("import-subagents");
        fx.transcript(
            "-work-app",
            "00000000-0000-4000-8000-000000000001",
            &[USER_AT_10],
        );
        fx.transcript("-work-app/s1/subagents", "agent-a1", &[USER_AT_10]);
        fx.transcript("subagents", "agent-a2", &[USER_AT_10]);
        assert_eq!(
            ids(&fx.scan()),
            vec!["00000000-0000-4000-8000-000000000001"]
        );
    }

    #[test]
    fn missing_roots_mean_no_sessions() {
        let fx = Fixture::new("import-empty");
        let outcome = fx.scan();
        assert!(outcome.candidates.is_empty());
        assert_eq!(outcome.skipped, 0);
    }

    #[test]
    fn known_sessions_are_excluded() {
        let fx = Fixture::new("import-known");
        fx.transcript(
            "-work-app",
            "00000000-0000-4000-8000-000000000001",
            &[USER_AT_10],
        );
        fx.transcript(
            "-work-app",
            "00000000-0000-4000-8000-000000000002",
            &[USER_AT_10],
        );
        let known = HashSet::from(["00000000-0000-4000-8000-000000000001".to_string()]);
        let outcome = scan(&fx.roots, &known).unwrap();
        assert_eq!(ids(&outcome), vec!["00000000-0000-4000-8000-000000000002"]);
    }

    #[test]
    fn candidates_are_sorted_newest_first() {
        let fx = Fixture::new("import-sort");
        fx.transcript(
            "-work-app",
            "00000000-0000-4000-8000-000000000021",
            &[r#"{"type":"user","timestamp":"2026-01-01T00:00:00Z","cwd":"/a"}"#],
        );
        fx.transcript(
            "-work-app",
            "00000000-0000-4000-8000-000000000022",
            &[r#"{"type":"user","timestamp":"2026-09-01T00:00:00Z","cwd":"/a"}"#],
        );
        assert_eq!(
            ids(&fx.scan()),
            vec![
                "00000000-0000-4000-8000-000000000022",
                "00000000-0000-4000-8000-000000000021"
            ]
        );
    }

    #[test]
    fn desktop_session_is_read_from_its_json_and_needs_a_transcript() {
        let fx = Fixture::new("import-desktop");
        fx.transcript(
            "-work-app",
            "00000000-0000-4000-8000-000000000011",
            &[USER_AT_10],
        );
        fx.desktop(
            "local_a.json",
            r#"{"sessionId":"local_a","cliSessionId":"00000000-0000-4000-8000-000000000011","cwd":"/work/app","title":"Desktop name","lastActivityAt":1791590500000,"isArchived":false}"#,
        );

        let outcome = fx.scan();
        assert_eq!(
            outcome.candidates,
            vec![ImportCandidate {
                session_id: "00000000-0000-4000-8000-000000000011".to_string(),
                source: ImportSource::Desktop,
                title: "Desktop name".to_string(),
                cwd: "/work/app".to_string(),
                last_activity: 1_791_590_500,
            }]
        );
    }

    #[test]
    fn desktop_wins_over_cli_on_overlap() {
        let fx = Fixture::new("import-overlap");
        fx.transcript(
            "-work-app",
            "00000000-0000-4000-8000-000000000011",
            &[
                USER_AT_10,
                r#"{"type":"custom-title","customTitle":"CLI name"}"#,
            ],
        );
        fx.transcript(
            "-work-app",
            "00000000-0000-4000-8000-000000000012",
            &[USER_AT_10],
        );
        fx.desktop(
            "local_a.json",
            r#"{"cliSessionId":"00000000-0000-4000-8000-000000000011","cwd":"/work/app","title":"Desktop name","lastActivityAt":1791590500000}"#,
        );

        let outcome = fx.scan();
        assert_eq!(
            ids(&outcome),
            vec![
                "00000000-0000-4000-8000-000000000011",
                "00000000-0000-4000-8000-000000000012"
            ]
        );
        assert_eq!(outcome.candidates[0].source, ImportSource::Desktop);
        assert_eq!(outcome.candidates[0].title, "Desktop name");
        assert_eq!(outcome.candidates[1].source, ImportSource::Cli);
    }

    #[test]
    fn archived_desktop_entries_are_skipped() {
        let fx = Fixture::new("import-archived");
        fx.transcript(
            "-work-app",
            "00000000-0000-4000-8000-000000000011",
            &[USER_AT_10],
        );
        fx.desktop(
            "local_a.json",
            r#"{"cliSessionId":"00000000-0000-4000-8000-000000000011","cwd":"/work/app","title":"T","lastActivityAt":1,"isArchived":true}"#,
        );
        fx.transcript(
            "-work-app",
            "00000000-0000-4000-8000-000000000012",
            &[USER_AT_10],
        );
        // The archive also hides the same session's CLI transcript.
        assert_eq!(
            ids(&fx.scan()),
            vec!["00000000-0000-4000-8000-000000000012"]
        );
    }

    #[test]
    fn desktop_entries_without_a_transcript_are_skipped() {
        let fx = Fixture::new("import-no-transcript");
        fx.desktop(
            "local_a.json",
            r#"{"cliSessionId":"00000000-0000-4000-8000-000000000031","cwd":"/work/app","title":"T","lastActivityAt":1}"#,
        );
        fx.desktop("local_b.json", r#"{"cwd":"/work/app","title":"never ran"}"#);
        let outcome = fx.scan();
        assert!(outcome.candidates.is_empty());
        assert_eq!(outcome.skipped, 0);
    }

    #[test]
    fn deleted_stubs_and_malformed_files_are_handled() {
        let fx = Fixture::new("import-stubs");
        fx.transcript(
            "-work-app",
            "00000000-0000-4000-8000-000000000011",
            &[USER_AT_10],
        );
        fx.desktop(
            "deleted_a.json",
            r#"{"cliSessionId":"00000000-0000-4000-8000-000000000011","cwd":"/work/app","title":"T","lastActivityAt":1}"#,
        );
        fx.desktop("local_bad.json", "not json {{{");
        let outcome = fx.scan();
        assert_eq!(outcome.candidates.len(), 1);
        assert_eq!(outcome.candidates[0].source, ImportSource::Cli);
        assert_eq!(outcome.skipped, 1);
    }

    #[test]
    fn desktop_without_a_title_borrows_the_transcripts() {
        let fx = Fixture::new("import-desktop-title");
        fx.transcript(
            "-work-app",
            "00000000-0000-4000-8000-000000000011",
            &[USER_AT_10, r#"{"type":"ai-title","aiTitle":"AI name"}"#],
        );
        fx.desktop(
            "local_a.json",
            r#"{"cliSessionId":"00000000-0000-4000-8000-000000000011","cwd":"/work/app","title":"","lastActivityAt":1791590500000}"#,
        );
        let outcome = fx.scan();
        assert_eq!(outcome.candidates[0].source, ImportSource::Desktop);
        assert_eq!(outcome.candidates[0].title, "AI name");
    }

    #[test]
    fn non_uuid_session_ids_are_skipped() {
        let fx = Fixture::new("import-non-uuid");
        fx.transcript("-work-app", "agent-a1b2c3d", &[USER_AT_10]);
        fx.transcript("-work-app", "s1; touch pwned", &[USER_AT_10]);
        fx.transcript(
            "-work-app",
            "00000000-0000-4000-8000-000000000001",
            &[USER_AT_10],
        );
        fx.desktop(
            "local_a.json",
            r#"{"cliSessionId":"not-a-uuid","cwd":"/work/app","title":"T","lastActivityAt":1}"#,
        );
        let outcome = fx.scan();
        assert_eq!(ids(&outcome), vec!["00000000-0000-4000-8000-000000000001"]);
        assert_eq!(outcome.skipped, 1);
    }

    #[test]
    fn desktop_entry_without_a_cwd_anywhere_is_skipped_and_counted() {
        let fx = Fixture::new("import-desktop-no-cwd");
        fx.transcript(
            "-work-app",
            "00000000-0000-4000-8000-000000000011",
            &[r#"{"type":"custom-title","customTitle":"T"}"#],
        );
        fx.desktop(
            "local_a.json",
            r#"{"cliSessionId":"00000000-0000-4000-8000-000000000011","title":"T","lastActivityAt":1}"#,
        );
        let outcome = fx.scan();
        assert!(outcome.candidates.is_empty());
        // The transcript itself (CLI scan) and the Desktop entry pointing at it.
        assert_eq!(outcome.skipped, 2);
    }

    #[test]
    fn desktop_entry_without_a_cwd_borrows_the_transcripts() {
        let fx = Fixture::new("import-desktop-cwd-from-transcript");
        fx.transcript(
            "-work-app",
            "00000000-0000-4000-8000-000000000011",
            &[USER_AT_10],
        );
        fx.desktop(
            "local_a.json",
            r#"{"cliSessionId":"00000000-0000-4000-8000-000000000011","title":"T","lastActivityAt":1}"#,
        );
        let candidate = &fx.scan().candidates[0];
        assert_eq!(candidate.source, ImportSource::Desktop);
        assert_eq!(candidate.cwd, "/work/app");
    }

    #[test]
    fn sources_map_to_origins() {
        assert_eq!(ImportSource::Cli.origin(), Origin::ImportedCli);
        assert_eq!(ImportSource::Desktop.origin(), Origin::ImportedDesktop);
    }
}
