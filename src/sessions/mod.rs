pub mod discover;
pub mod import;
pub mod live;
pub mod record;
pub mod repo;
pub mod view;

use crate::multiplexer::PaneRef;
use anyhow::Result;
use live::{Closing, LiveSession, LiveSet, LiveStatus, Mutation, Probe, PromptState, Tick};
use record::{SessionRecord, Store};
use std::collections::{HashMap, HashSet};

pub fn basename(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Whether `id` has the 8-4-4-4-12 hex shape of the UUIDs Claude uses as session ids.
pub fn is_session_uuid(id: &str) -> bool {
    const GROUPS: [usize; 5] = [8, 4, 4, 4, 12];
    let mut parts = id.split('-');
    GROUPS.iter().all(|&len| {
        parts
            .next()
            .is_some_and(|part| part.len() == len && part.bytes().all(|b| b.is_ascii_hexdigit()))
    }) && parts.next().is_none()
}

/// The shell command that resumes `session_id`. Refuses ids that aren't UUIDs because the result
/// is run through a shell.
pub fn resume_command(command: &str, session_id: &str) -> Result<String> {
    anyhow::ensure!(
        is_session_uuid(session_id),
        "Refusing to resume: {session_id:?} is not a session UUID"
    );
    Ok(format!("{command} --resume {}", shell_quote(session_id)))
}

fn shell_quote(arg: &str) -> String {
    format!("'{}'", arg.replace('\'', "'\\''"))
}

/// The title, or the first 8 characters of the session id for a session that was never named.
pub fn display_title(title: &str, session_id: &str) -> String {
    if title.is_empty() {
        session_id.chars().take(8).collect()
    } else {
        title.to_string()
    }
}

/// What the list shows: the persisted record plus the live overlay when a process is running.
#[derive(Debug, Clone)]
pub struct Session {
    pub record: SessionRecord,
    pub live: Option<LiveSession>,
}

impl Session {
    pub fn is_live(&self) -> bool {
        self.live.is_some()
    }

    pub fn is_busy(&self) -> bool {
        self.live
            .as_ref()
            .is_some_and(|live| live.status == LiveStatus::Busy)
    }

    /// A live session waiting on the user: Claude says so, or the pane shows a plan/question.
    pub fn needs_action(&self) -> bool {
        self.live.as_ref().is_some_and(|live| {
            live.status == LiveStatus::Waiting || live.prompt_state != PromptState::None
        })
    }

    pub fn prompt_state(&self) -> PromptState {
        self.live
            .as_ref()
            .map_or(PromptState::None, |live| live.prompt_state)
    }

    pub fn pane(&self) -> Option<&PaneRef> {
        self.live.as_ref().map(|live| &live.pane)
    }

    pub fn display_title(&self) -> String {
        display_title(&self.record.title, &self.record.session_id)
    }

    pub fn folder_name(&self) -> String {
        basename(&self.record.cwd)
    }
}

/// The persisted store plus the live sessions of the current and previous tick.
pub struct SessionBook {
    store: Store,
    live: LiveSet,
    prev_live: Option<LiveSet>,
    closing: Closing,
    /// Panes spawned for `c` and when (epoch ms), until a session shows up on them or
    /// `CREATED_PANE_TTL_MS` passes (the command failed to start).
    created_panes: HashMap<String, u64>,
    /// When a save was last attempted, successful or not (epoch ms).
    last_save_attempt_ms: u64,
    /// A save failed and none has succeeded since; keeps the error from being reported every tick.
    save_failing: bool,
    auto_track: bool,
}

/// How long a spawned pane is remembered while waiting for its session to appear.
const CREATED_PANE_TTL_MS: u64 = 120_000;

/// Metadata-only changes (title, cwd, last activity) are written at most this often.
const METADATA_SAVE_INTERVAL_MS: u64 = 10_000;

impl SessionBook {
    pub fn new(store: Store) -> SessionBook {
        SessionBook {
            store,
            live: LiveSet::default(),
            prev_live: None,
            closing: Closing::default(),
            created_panes: HashMap::new(),
            last_save_attempt_ms: 0,
            save_failing: false,
            auto_track: true,
        }
    }

    /// Set from the config before each `update`, so a config reload takes effect on the next tick.
    pub fn set_auto_track(&mut self, auto_track: bool) {
        self.auto_track = auto_track;
    }

    /// Marks a pane we spawned so the session that appears on it is recorded as `Created`.
    pub fn expect_created(&mut self, pane_id: String, now_ms: u64) {
        self.created_panes.insert(pane_id, now_ms);
    }

    pub fn record(&self, session_id: &str) -> Option<&SessionRecord> {
        self.store.get(session_id)
    }

    pub fn sessions(&self) -> Vec<Session> {
        self.store
            .records()
            .map(|record| Session {
                record: record.clone(),
                live: self.live.get(&record.session_id).cloned(),
            })
            .collect()
    }

    /// Applies one discovery tick. `discovered` is `None` when the multiplexer was unreachable.
    ///
    /// The in-memory state is updated even when saving fails, so one bad write doesn't wedge
    /// tracking; the failure is retried on the next tick and returned once per failure streak
    /// for the caller to surface.
    pub fn update(
        &mut self,
        discovered: Option<LiveSet>,
        probe: &dyn Probe,
        now_ms: u64,
    ) -> Result<()> {
        let reachable = discovered.is_some();
        let current = discovered.unwrap_or_default();

        self.created_panes
            .retain(|_, spawned_at| now_ms.saturating_sub(*spawned_at) < CREATED_PANE_TTL_MS);
        let created_panes: HashSet<String> = self.created_panes.keys().cloned().collect();
        let mutations = live::track(
            &Tick {
                prev: self.prev_live.as_ref(),
                current: &current,
                multiplexer_reachable: reachable,
                auto_track: self.auto_track,
                now_ms,
            },
            &self.store,
            probe,
            &mut self.closing,
            &created_panes,
        );
        // Once a session is on the pane the origin decision has been made.
        self.created_panes
            .retain(|pane_id, _| !current.values().any(|s| s.pane.pane_id == *pane_id));
        live::apply(&mut self.store, mutations, now_ms);
        let refresh = live::refresh_metadata(&current, &self.store);
        live::apply(&mut self.store, refresh, now_ms);

        self.live = current.clone();
        // Kept across an unreachable tick: closes that happened during the outage are
        // classified once the multiplexer answers again.
        if reachable {
            self.prev_live = Some(current);
        }

        if self.save_due(now_ms) {
            self.save(now_ms)
        } else {
            self.store.sync()
        }
    }

    fn save_due(&self, now_ms: u64) -> bool {
        self.store.has_unsaved_intent()
            || (self.store.has_unsaved_metadata()
                && now_ms.saturating_sub(self.last_save_attempt_ms) >= METADATA_SAVE_INTERVAL_MS)
    }

    fn save(&mut self, now_ms: u64) -> Result<()> {
        self.last_save_attempt_ms = now_ms;
        match self.store.save(now_ms) {
            Ok(()) => {
                self.save_failing = false;
                Ok(())
            }
            // Already reported when the streak began.
            Err(_) if self.save_failing => Ok(()),
            Err(err) => {
                self.save_failing = true;
                Err(err)
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.store.records().next().is_none()
    }

    /// Writes anything still waiting, e.g. debounced metadata, before exiting.
    pub fn flush(&mut self, now_ms: u64) -> Result<()> {
        if self.store.has_unsaved_intent() || self.store.has_unsaved_metadata() {
            self.save(now_ms)?;
        }
        Ok(())
    }

    /// For user-driven changes (mark unread, archive, ...), saved immediately.
    pub fn mutate(&mut self, mutations: Vec<Mutation>, now_ms: u64) -> Result<()> {
        live::apply(&mut self.store, mutations, now_ms);
        if self.store.has_unsaved_intent() {
            self.save(now_ms)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use live::PidFileState;
    use record::tests::TempDir;
    use record::ArchiveReason;
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;

    const UUID: &str = "0b9c1f2e-3a4d-4e5f-8a6b-7c8d9e0f1a2b";

    #[test]
    fn session_uuids_are_8_4_4_4_12_hex() {
        assert!(is_session_uuid(UUID));
        assert!(is_session_uuid(&UUID.to_uppercase()));
        for bad in [
            "",
            "s1",
            "agent-a1b2c3d",
            "0b9c1f2e3a4d4e5f8a6b7c8d9e0f1a2b",
            "0b9c1f2e-3a4d-4e5f-8a6b-7c8d9e0f1a2",
            "0b9c1f2e-3a4d-4e5f-8a6b-7c8d9e0f1a2b0",
            "0b9c1f2e-3a4d-4e5f-8a6b-7c8d9e0f1a2b-1",
            "0b9c1f2e-3a4d-4e5f-8a6b-7c8d9e0f1a2g",
            "0b9c1f2e-3a4d-4e5f-8a6b-7c8d9e0f1a2é",
            "0b9c1f2e-3a4d-4e5f-8a6b-7c8d9e0f1a2b'; rm -rf ~ #",
        ] {
            assert!(!is_session_uuid(bad), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn resume_command_quotes_the_session_id() {
        assert_eq!(
            resume_command("claude", UUID).unwrap(),
            format!("claude --resume '{UUID}'")
        );
    }

    #[test]
    fn resume_command_refuses_non_uuids() {
        let err = resume_command("claude", "x; touch /tmp/pwned").unwrap_err();
        assert!(err.to_string().contains("not a session UUID"));
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("a'b"), r#"'a'\''b'"#);
    }

    #[derive(Default)]
    struct FakeProbe {
        pid_files: HashMap<u32, String>,
        transcripts: HashSet<String>,
    }

    impl Probe for FakeProbe {
        fn pid_file(&self, pid: u32) -> PidFileState {
            self.pid_files
                .get(&pid)
                .cloned()
                .map_or(PidFileState::Missing, PidFileState::Session)
        }

        fn pid_alive(&self, _pid: u32) -> bool {
            false
        }

        fn has_transcript(&self, session_id: &str) -> bool {
            self.transcripts.contains(session_id)
        }
    }

    fn live_set_started(
        id: &str,
        status: LiveStatus,
        started_at: u64,
        last_activity: u64,
    ) -> LiveSet {
        LiveSet::collapse(vec![LiveSession {
            session_id: id.to_string(),
            pid: 7,
            pane: PaneRef {
                pane_id: "%1".to_string(),
                target: "main:0.0".to_string(),
            },
            status,
            prompt_state: PromptState::None,
            name: Some("my session".to_string()),
            cwd: "/work/app".to_string(),
            started_at: Some(started_at),
            last_activity: Some(last_activity),
        }])
    }

    fn live_set(id: &str, status: LiveStatus) -> LiveSet {
        live_set_started(id, status, 10_000, 20)
    }

    fn probe_with_transcript(id: &str) -> FakeProbe {
        FakeProbe {
            transcripts: HashSet::from([id.to_string()]),
            ..FakeProbe::default()
        }
    }

    fn on_disk(dir: &TempDir) -> Store {
        Store::load(dir.store_path()).unwrap().unwrap()
    }

    #[test]
    fn lifecycle_insert_unread_close_and_resume() {
        let dir = TempDir::new("book");
        let mut book = SessionBook::new(Store::empty(dir.store_path()));
        let probe = probe_with_transcript("s");

        book.update(Some(live_set("s", LiveStatus::Busy)), &probe, 100_000)
            .unwrap();
        let sessions = book.sessions();
        assert_eq!(sessions.len(), 1);
        assert!(sessions[0].is_busy());
        assert_eq!(sessions[0].record.title, "my session");

        book.update(Some(live_set("s", LiveStatus::Idle)), &probe, 110_000)
            .unwrap();
        assert!(book.sessions()[0].record.unread);

        book.update(Some(LiveSet::default()), &probe, 120_000)
            .unwrap();
        let closed = &book.sessions()[0];
        assert!(!closed.is_live());
        assert_eq!(closed.record.archived_at, Some(120));
        assert_eq!(closed.record.archive_reason, Some(ArchiveReason::Closed));
        assert!(!closed.record.unread, "archiving clears unread");

        // The same old process showing up again is a blip, not a resume.
        book.update(Some(live_set("s", LiveStatus::Idle)), &probe, 125_000)
            .unwrap();
        assert!(book.sessions()[0].record.archived_at.is_some());

        let resumed = live_set_started("s", LiveStatus::Idle, 128_000, 20);
        book.update(Some(resumed), &probe, 130_000).unwrap();
        assert_eq!(book.sessions()[0].record.archived_at, None);
        assert_eq!(book.sessions()[0].record.archive_reason, None);

        let reloaded = on_disk(&dir);
        assert_eq!(reloaded.get("s").unwrap().archived_at, None);
    }

    #[test]
    fn session_appearing_on_a_spawned_pane_is_recorded_as_created() {
        let dir = TempDir::new("book-created");
        let mut book = SessionBook::new(Store::empty(dir.store_path()));
        let probe = probe_with_transcript("s");

        book.expect_created("%1".to_string(), 100_000);
        // Claude hasn't written its pid file yet.
        book.update(Some(LiveSet::default()), &probe, 101_000)
            .unwrap();
        book.update(Some(live_set("s", LiveStatus::Idle)), &probe, 103_000)
            .unwrap();

        assert_eq!(book.sessions()[0].record.origin, record::Origin::Created);
        assert!(book.created_panes.is_empty());
    }

    #[test]
    fn spawned_pane_that_never_starts_a_session_expires() {
        let dir = TempDir::new("book-created-expiry");
        let mut book = SessionBook::new(Store::empty(dir.store_path()));
        let probe = probe_with_transcript("s");

        book.expect_created("%1".to_string(), 100_000);
        book.update(
            Some(LiveSet::default()),
            &probe,
            100_000 + CREATED_PANE_TTL_MS,
        )
        .unwrap();
        book.update(Some(live_set("s", LiveStatus::Idle)), &probe, 400_000)
            .unwrap();

        assert_eq!(book.sessions()[0].record.origin, record::Origin::Detected);
    }

    #[test]
    fn unreachable_tick_shows_everything_closed_but_does_not_archive() {
        let dir = TempDir::new("book-unreachable");
        let mut book = SessionBook::new(Store::empty(dir.store_path()));
        let probe = probe_with_transcript("s");

        book.update(Some(live_set("s", LiveStatus::Idle)), &probe, 100_000)
            .unwrap();
        book.update(None, &probe, 110_000).unwrap();

        let session = &book.sessions()[0];
        assert!(!session.is_live());
        assert_eq!(session.record.archived_at, None);

        // Back again, still the same process: nothing to undo or redo.
        book.update(Some(live_set("s", LiveStatus::Idle)), &probe, 120_000)
            .unwrap();
        assert!(book.sessions()[0].is_live());
        assert_eq!(book.sessions()[0].record.archived_at, None);
    }

    #[test]
    fn close_during_an_outage_is_archived_once_tmux_answers() {
        let dir = TempDir::new("book-outage-close");
        let mut book = SessionBook::new(Store::empty(dir.store_path()));
        let probe = probe_with_transcript("s");

        book.update(Some(live_set("s", LiveStatus::Idle)), &probe, 100_000)
            .unwrap();
        book.update(None, &probe, 110_000).unwrap();
        book.update(Some(LiveSet::default()), &probe, 120_000)
            .unwrap();

        assert_eq!(book.sessions()[0].record.archived_at, Some(120));
    }

    #[test]
    fn first_tick_after_startup_leaves_sessions_that_closed_while_down() {
        let dir = TempDir::new("book-restart");
        let mut store = Store::empty(dir.store_path());
        store.insert(record::tests::record("s", 1));
        store.save(2).unwrap();

        let mut book = SessionBook::new(on_disk(&dir));
        book.update(
            Some(LiveSet::default()),
            &probe_with_transcript("s"),
            500_000,
        )
        .unwrap();

        assert_eq!(book.sessions()[0].record.archived_at, None);
    }

    #[test]
    fn session_without_transcript_is_deleted_on_close() {
        let dir = TempDir::new("book-delete");
        let mut book = SessionBook::new(Store::empty(dir.store_path()));
        let probe = FakeProbe::default();

        book.update(Some(live_set("s", LiveStatus::Idle)), &probe, 100_000)
            .unwrap();
        book.update(Some(LiveSet::default()), &probe, 110_000)
            .unwrap();

        assert!(book.sessions().is_empty());
        let reloaded = on_disk(&dir);
        assert_eq!(reloaded.records().count(), 0);
        assert!(reloaded.has_tombstone("s"));
    }

    #[test]
    fn mutate_saves_user_changes() {
        let dir = TempDir::new("book-mutate");
        let mut book = SessionBook::new(Store::empty(dir.store_path()));
        let probe = FakeProbe::default();
        book.update(Some(live_set("s", LiveStatus::Idle)), &probe, 100_000)
            .unwrap();

        book.mutate(
            vec![Mutation::SetUnread {
                session_id: "s".to_string(),
                unread: true,
            }],
            105_000,
        )
        .unwrap();

        assert!(on_disk(&dir).get("s").unwrap().unread);
    }

    #[test]
    fn user_archive_is_saved_immediately_with_the_user_reason() {
        let dir = TempDir::new("book-user-archive");
        let mut book = SessionBook::new(Store::empty(dir.store_path()));
        book.update(
            Some(live_set("s", LiveStatus::Idle)),
            &FakeProbe::default(),
            100_000,
        )
        .unwrap();

        book.mutate(
            vec![Mutation::Archive {
                session_id: "s".to_string(),
                reason: ArchiveReason::User,
            }],
            105_000,
        )
        .unwrap();

        let saved = on_disk(&dir);
        assert_eq!(
            saved.get("s").unwrap().archive_reason,
            Some(ArchiveReason::User)
        );
        assert_eq!(saved.get("s").unwrap().archive_changed_at, 105_000);
    }

    // -- saving --

    fn last_activity_on_disk(dir: &TempDir) -> u64 {
        on_disk(dir).get("s").unwrap().last_activity
    }

    #[test]
    fn metadata_only_changes_are_saved_at_most_every_10_seconds() {
        let dir = TempDir::new("book-debounce");
        let mut book = SessionBook::new(Store::empty(dir.store_path()));
        let probe = FakeProbe::default();
        book.update(
            Some(live_set_started("s", LiveStatus::Idle, 10_000, 20)),
            &probe,
            100_000,
        )
        .unwrap();
        assert_eq!(
            last_activity_on_disk(&dir),
            20,
            "insert is intent: saved at once"
        );

        book.update(
            Some(live_set_started("s", LiveStatus::Idle, 10_000, 30)),
            &probe,
            102_000,
        )
        .unwrap();
        book.update(
            Some(live_set_started("s", LiveStatus::Idle, 10_000, 40)),
            &probe,
            105_000,
        )
        .unwrap();
        assert_eq!(book.sessions()[0].record.last_activity, 40);
        assert_eq!(last_activity_on_disk(&dir), 20, "held back");

        book.update(
            Some(live_set_started("s", LiveStatus::Idle, 10_000, 40)),
            &probe,
            110_000,
        )
        .unwrap();
        assert_eq!(last_activity_on_disk(&dir), 40, "10 s after the last save");
    }

    #[test]
    fn an_intent_change_flushes_pending_metadata_with_it() {
        let dir = TempDir::new("book-intent-flush");
        let mut book = SessionBook::new(Store::empty(dir.store_path()));
        let probe = FakeProbe::default();
        book.update(
            Some(live_set_started("s", LiveStatus::Idle, 10_000, 20)),
            &probe,
            100_000,
        )
        .unwrap();
        book.update(
            Some(live_set_started("s", LiveStatus::Idle, 10_000, 30)),
            &probe,
            102_000,
        )
        .unwrap();
        assert_eq!(last_activity_on_disk(&dir), 20);

        book.mutate(
            vec![Mutation::SetUnread {
                session_id: "s".to_string(),
                unread: true,
            }],
            103_000,
        )
        .unwrap();
        assert_eq!(last_activity_on_disk(&dir), 30);
    }

    #[test]
    fn flush_writes_held_back_metadata() {
        let dir = TempDir::new("book-flush");
        let mut book = SessionBook::new(Store::empty(dir.store_path()));
        let probe = FakeProbe::default();
        book.update(
            Some(live_set_started("s", LiveStatus::Idle, 10_000, 20)),
            &probe,
            100_000,
        )
        .unwrap();
        book.update(
            Some(live_set_started("s", LiveStatus::Idle, 10_000, 30)),
            &probe,
            101_000,
        )
        .unwrap();

        book.flush(101_500).unwrap();
        assert_eq!(last_activity_on_disk(&dir), 30);
    }

    /// A store path whose directory can't be created while `blocker` is a regular file.
    fn blocked_store(dir: &TempDir) -> (PathBuf, PathBuf) {
        let blocker = dir.0.join("blocker");
        std::fs::write(&blocker, "").unwrap();
        (blocker.clone(), blocker.join("sessions.json"))
    }

    #[test]
    fn failed_save_is_retried_every_tick_until_it_succeeds() {
        let dir = TempDir::new("book-retry");
        let (blocker, path) = blocked_store(&dir);
        let mut book = SessionBook::new(Store::empty(path.clone()));
        let probe = FakeProbe::default();
        let session = || live_set("s", LiveStatus::Idle);

        assert!(book.update(Some(session()), &probe, 100_000).is_err());
        // Nothing new to save on this tick; the earlier failure is still pending.
        assert!(book.update(Some(session()), &probe, 102_000).is_ok());
        assert!(!path.exists());

        std::fs::remove_file(&blocker).unwrap();
        book.update(Some(session()), &probe, 104_000).unwrap();
        assert!(on_disk_at(&path).get("s").is_some());
    }

    fn on_disk_at(path: &std::path::Path) -> Store {
        Store::load(path.to_path_buf()).unwrap().unwrap()
    }

    #[test]
    fn write_error_is_reported_once_per_failure_streak() {
        let dir = TempDir::new("book-error-once");
        let (blocker, path) = blocked_store(&dir);
        let mut book = SessionBook::new(Store::empty(path));
        let probe = FakeProbe::default();
        let session = || live_set("s", LiveStatus::Idle);

        assert!(book.update(Some(session()), &probe, 100_000).is_err());
        assert!(book.update(Some(session()), &probe, 102_000).is_ok());
        assert!(book.update(Some(session()), &probe, 104_000).is_ok());

        // Recovers, then fails again: a new streak is reported again.
        std::fs::remove_file(&blocker).unwrap();
        book.update(Some(session()), &probe, 106_000).unwrap();
        let store_path = blocker.join("sessions.json");
        assert!(store_path.is_file());
        std::fs::remove_file(&store_path).unwrap();
        std::fs::create_dir(&store_path).unwrap();
        let set_unread = || {
            vec![Mutation::SetUnread {
                session_id: "s".to_string(),
                unread: true,
            }]
        };
        assert!(book.mutate(set_unread(), 108_000).is_err());
    }

    #[test]
    fn unchanged_file_is_not_read_again_on_quiet_ticks() {
        let dir = TempDir::new("book-sync-gate");
        let mut book = SessionBook::new(Store::empty(dir.store_path()));
        let probe = FakeProbe::default();
        let session = || live_set("s", LiveStatus::Idle);
        book.update(Some(session()), &probe, 100_000).unwrap();

        // Same length would be needed to fool the stat check; a different length is picked up.
        let mut other = on_disk(&dir);
        other.insert(record::tests::record("other", 5));
        other.save(6).unwrap();
        book.update(Some(session()), &probe, 102_000).unwrap();
        assert_eq!(book.sessions().len(), 2);
    }

    #[test]
    fn display_title_falls_back_to_short_session_id() {
        let mut session = Session {
            record: record::tests::record("0123456789abcdef", 1),
            live: None,
        };
        session.record.title = String::new();
        assert_eq!(session.display_title(), "01234567");
        session.record.title = "named".to_string();
        assert_eq!(session.display_title(), "named");
    }
}
