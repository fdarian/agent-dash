use super::record::{ArchiveReason, ArchiveSnapshot, Origin, SessionRecord, Store};
use crate::multiplexer::PaneRef;
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveStatus {
    Busy,
    /// Claude is waiting on the user: a permission prompt or a question.
    Waiting,
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptState {
    None,
    Plan,
    Ask,
}

pub fn detect_prompt_state(visible_text: &str) -> PromptState {
    let last_line = visible_text.lines().rev().find(|l| !l.trim().is_empty());
    match last_line {
        Some(line) if line.contains("ctrl-g to edit") => PromptState::Plan,
        Some(line) if line.contains("Enter to select") => PromptState::Ask,
        _ => PromptState::None,
    }
}

/// A session with a running Claude process, as seen on one tick. Never persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveSession {
    pub session_id: String,
    pub pid: u32,
    pub pane: PaneRef,
    pub status: LiveStatus,
    pub prompt_state: PromptState,
    /// The title to show: the user's name, else the transcript's title, else `derived_name`.
    /// Discovery only knows the user's name; polling fills in the rest.
    pub name: Option<String>,
    /// The name Claude generated, the last resort once the transcript has no title.
    pub derived_name: Option<String>,
    pub cwd: String,
    /// Epoch milliseconds, as in the pid file.
    pub started_at: Option<u64>,
    pub last_activity: Option<u64>,
}

/// One tick's live sessions, one entry per session id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiveSet {
    sessions: BTreeMap<String, LiveSession>,
}

impl LiveSet {
    /// The same session resumed in several panes collapses into one entry: the busy pane wins,
    /// then the most recently started process.
    pub fn collapse(sessions: Vec<LiveSession>) -> LiveSet {
        let mut collapsed: BTreeMap<String, LiveSession> = BTreeMap::new();
        for session in sessions {
            match collapsed.get(&session.session_id) {
                Some(kept) if preference(kept) >= preference(&session) => {}
                _ => {
                    collapsed.insert(session.session_id.clone(), session);
                }
            }
        }
        LiveSet {
            sessions: collapsed,
        }
    }

    pub fn get(&self, session_id: &str) -> Option<&LiveSession> {
        self.sessions.get(session_id)
    }

    pub fn contains(&self, session_id: &str) -> bool {
        self.sessions.contains_key(session_id)
    }

    pub fn values(&self) -> impl Iterator<Item = &LiveSession> {
        self.sessions.values()
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut LiveSession> {
        self.sessions.values_mut()
    }
}

fn preference(session: &LiveSession) -> (bool, Option<u64>) {
    (session.status == LiveStatus::Busy, session.started_at)
}

/// What `~/.claude/sessions/<pid>.json` holds, as far as `track` cares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PidFileState {
    Missing,
    /// Present but unreadable or unparseable, e.g. caught mid-write.
    Unreadable,
    Session(String),
}

/// What `track` needs to know about the outside world, so it stays free of IO.
pub trait Probe {
    fn pid_file(&self, pid: u32) -> PidFileState;
    fn pid_alive(&self, pid: u32) -> bool;
    /// Whether `~/.claude/projects/*/<id>.jsonl` exists, i.e. the session was ever prompted.
    fn has_transcript(&self, session_id: &str) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mutation {
    Insert(SessionRecord),
    Archive {
        session_id: String,
        reason: ArchiveReason,
    },
    Unarchive {
        session_id: String,
    },
    SetUnread {
        session_id: String,
        unread: bool,
    },
    /// Undo of an archive or unarchive.
    RestoreArchive {
        session_id: String,
        snapshot: ArchiveSnapshot,
    },
    Delete {
        session_id: String,
    },
    SetRepoRoot {
        session_id: String,
        repo_root: String,
    },
    /// Fields are `Some` only when they differ from the stored record.
    Refresh {
        session_id: String,
        title: Option<String>,
        cwd: Option<String>,
        last_activity: Option<u64>,
    },
}

/// How long a session whose process is still alive may keep its pid file before we stop waiting
/// for Claude to clean it up.
const CLOSE_GRACE_MS: u64 = 10_000;

struct PendingClose {
    pid: u32,
    since: u64,
}

/// Sessions that vanished from the multiplexer but whose process is still exiting: pid file
/// present with the same id and the pid alive. Carried between ticks by the caller.
#[derive(Default)]
pub struct Closing {
    pending: BTreeMap<String, PendingClose>,
}

enum CloseOutcome {
    /// Closed on purpose: pid file gone, or taken over by another session.
    Explicit,
    /// Pid file left behind by a dead process.
    Crash,
    /// Process still alive and the file still there, or the file can't be read right now;
    /// ask again next tick.
    Pending,
}

fn classify_close(probe: &dyn Probe, pid: u32, session_id: &str) -> CloseOutcome {
    match probe.pid_file(pid) {
        PidFileState::Session(id) if id == session_id => {
            if probe.pid_alive(pid) {
                CloseOutcome::Pending
            } else {
                CloseOutcome::Crash
            }
        }
        PidFileState::Unreadable => CloseOutcome::Pending,
        PidFileState::Session(_) | PidFileState::Missing => CloseOutcome::Explicit,
    }
}

/// One discovery tick as seen by `track`.
pub struct Tick<'a> {
    /// `None` on the first tick after startup.
    pub prev: Option<&'a LiveSet>,
    pub current: &'a LiveSet,
    pub multiplexer_reachable: bool,
    /// Whether newly detected sessions are inserted. When false, only sessions on a pane in
    /// `created_panes` are; ones already in the store are tracked either way.
    pub auto_track: bool,
    /// Epoch milliseconds.
    pub now_ms: u64,
}

/// State transitions between two ticks. Metadata refresh is deliberately separate
/// (`refresh_metadata`).
///
/// Close transitions need both a `prev` and a reachable multiplexer: otherwise an empty
/// `current` would archive everything.
///
/// `closing` is the caller-owned set of sessions still waiting on their process to exit; it is
/// re-examined on every reachable tick until each resolves or `CLOSE_GRACE_MS` passes.
///
/// `created_panes` are panes agent-dash spawned for `c`; a new session appearing on one is
/// inserted with `Origin::Created` instead of `Origin::Detected`.
pub fn track(
    tick: &Tick,
    store: &Store,
    probe: &dyn Probe,
    closing: &mut Closing,
    created_panes: &HashSet<String>,
) -> Vec<Mutation> {
    let prev = tick.prev;
    let current = tick.current;
    let multiplexer_reachable = tick.multiplexer_reachable;
    let now_ms = tick.now_ms;
    let mut mutations = Vec::new();

    for live in current.values() {
        let Some(record) = store.get(&live.session_id) else {
            let origin = if created_panes.contains(&live.pane.pane_id) {
                Origin::Created
            } else if tick.auto_track {
                Origin::Detected
            } else {
                continue;
            };
            mutations.push(Mutation::Insert(new_record(live, origin, now_ms)));
            continue;
        };

        // Only a process that started after the archive counts as a resume. Appearing in the
        // live set doesn't: a one-tick discovery blip must not undo the user's archive, and
        // archiving a session that is already live sticks.
        let resumed_after_archive = record.archived_at.is_some()
            && live
                .started_at
                .is_some_and(|started| started > record.archive_changed_at);
        if resumed_after_archive {
            mutations.push(Mutation::Unarchive {
                session_id: live.session_id.clone(),
            });
        }

        let was_busy = prev
            .and_then(|prev| prev.get(&live.session_id))
            .is_some_and(|before| before.status == LiveStatus::Busy);
        if was_busy
            && live.status == LiveStatus::Idle
            && !record.unread
            && record.archived_at.is_none()
        {
            mutations.push(Mutation::SetUnread {
                session_id: live.session_id.clone(),
                unread: true,
            });
        }
    }

    // Reappearing live cancels a pending close even while the multiplexer is unreachable.
    closing
        .pending
        .retain(|session_id, _| !current.contains(session_id));

    if multiplexer_reachable {
        let mut candidates: Vec<(String, u32)> = closing
            .pending
            .iter()
            .map(|(id, pending)| (id.clone(), pending.pid))
            .collect();
        if let Some(prev) = prev {
            candidates.extend(
                prev.values()
                    .filter(|s| !current.contains(&s.session_id))
                    .filter(|s| !closing.pending.contains_key(&s.session_id))
                    .map(|s| (s.session_id.clone(), s.pid)),
            );
        }

        for (session_id, pid) in candidates {
            let Some(record) = store.get(&session_id) else {
                closing.pending.remove(&session_id);
                continue;
            };
            match classify_close(probe, pid, &session_id) {
                CloseOutcome::Crash => {
                    closing.pending.remove(&session_id);
                }
                CloseOutcome::Pending => {
                    let since = closing
                        .pending
                        .entry(session_id.clone())
                        .or_insert(PendingClose { pid, since: now_ms })
                        .since;
                    if now_ms.saturating_sub(since) >= CLOSE_GRACE_MS {
                        closing.pending.remove(&session_id);
                    }
                }
                CloseOutcome::Explicit => {
                    closing.pending.remove(&session_id);
                    if !probe.has_transcript(&session_id) {
                        mutations.push(Mutation::Delete { session_id });
                    } else if record.archived_at.is_none() {
                        mutations.push(Mutation::Archive {
                            session_id,
                            reason: ArchiveReason::Closed,
                        });
                    }
                }
            }
        }
    }

    mutations
}

fn new_record(live: &LiveSession, origin: Origin, now_ms: u64) -> SessionRecord {
    SessionRecord::new(
        live.session_id.clone(),
        live.cwd.clone(),
        live.name.clone().unwrap_or_default(),
        now_ms / 1000,
        live.last_activity.unwrap_or(now_ms / 1000),
        origin,
        now_ms,
    )
}

pub fn refresh_metadata(current: &LiveSet, store: &Store) -> Vec<Mutation> {
    current
        .values()
        .filter_map(|live| {
            let record = store.get(&live.session_id)?;
            let title = live.name.clone().filter(|name| *name != record.title);
            let cwd = Some(live.cwd.clone()).filter(|cwd| *cwd != record.cwd);
            // Never lowered: the metadata merge keeps the larger value, so a lower one would be
            // undone on the next save and re-applied on the next tick.
            let last_activity = live
                .last_activity
                .filter(|activity| *activity > record.last_activity);
            if title.is_none() && cwd.is_none() && last_activity.is_none() {
                return None;
            }
            Some(Mutation::Refresh {
                session_id: live.session_id.clone(),
                title,
                cwd,
                last_activity,
            })
        })
        .collect()
}

pub fn apply(store: &mut Store, mutations: Vec<Mutation>, now_ms: u64) -> bool {
    let mut changed = false;
    for mutation in mutations {
        changed |= match mutation {
            Mutation::Insert(record) => {
                if store.get(&record.session_id).is_some() {
                    false
                } else {
                    store.insert(record);
                    true
                }
            }
            Mutation::Archive { session_id, reason } => store.archive(&session_id, reason, now_ms),
            Mutation::Unarchive { session_id } => store.unarchive(&session_id, now_ms),
            Mutation::SetUnread { session_id, unread } => {
                store.set_unread(&session_id, unread, now_ms)
            }
            Mutation::RestoreArchive {
                session_id,
                snapshot,
            } => store.restore_archive(&session_id, snapshot, now_ms),
            Mutation::Delete { session_id } => store.remove(&session_id, now_ms),
            Mutation::SetRepoRoot {
                session_id,
                repo_root,
            } => store.set_repo_root(&session_id, repo_root),
            Mutation::Refresh {
                session_id,
                title,
                cwd,
                last_activity,
            } => store.refresh(&session_id, title, cwd, last_activity),
        };
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::record::tests::record;
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;

    /// Epoch ms.
    const NOW: u64 = 1_000_000;

    #[derive(Default)]
    struct FakeProbe {
        pid_files: HashMap<u32, PidFileState>,
        transcripts: HashSet<String>,
        alive: HashSet<u32>,
    }

    impl FakeProbe {
        fn with_transcript(mut self, id: &str) -> Self {
            self.transcripts.insert(id.to_string());
            self
        }

        fn with_alive(mut self, pid: u32) -> Self {
            self.alive.insert(pid);
            self
        }

        fn with_pid_file(mut self, pid: u32, id: &str) -> Self {
            self.pid_files
                .insert(pid, PidFileState::Session(id.to_string()));
            self
        }

        fn with_unreadable_pid_file(mut self, pid: u32) -> Self {
            self.pid_files.insert(pid, PidFileState::Unreadable);
            self
        }
    }

    impl Probe for FakeProbe {
        fn pid_file(&self, pid: u32) -> PidFileState {
            self.pid_files
                .get(&pid)
                .cloned()
                .unwrap_or(PidFileState::Missing)
        }

        fn pid_alive(&self, pid: u32) -> bool {
            self.alive.contains(&pid)
        }

        fn has_transcript(&self, session_id: &str) -> bool {
            self.transcripts.contains(session_id)
        }
    }

    fn live(id: &str, pid: u32, status: LiveStatus) -> LiveSession {
        LiveSession {
            session_id: id.to_string(),
            pid,
            pane: PaneRef {
                pane_id: format!("%{pid}"),
                target: format!("main:0.{pid}"),
            },
            status,
            prompt_state: PromptState::None,
            name: Some(format!("name {id}")),
            derived_name: None,
            cwd: "/work/app".to_string(),
            started_at: Some(500_000),
            last_activity: Some(900),
        }
    }

    fn set(sessions: Vec<LiveSession>) -> LiveSet {
        LiveSet::collapse(sessions)
    }

    fn store_with(records: Vec<SessionRecord>) -> Store {
        let mut store = Store::empty(PathBuf::from("/nonexistent/sessions.json"));
        for record in records {
            store.insert(record);
        }
        store
    }

    /// Archived by the user at `at_ms`.
    fn archived(id: &str, at_ms: u64) -> SessionRecord {
        let mut r = record(id, 1);
        r.archived_at = Some(at_ms / 1000);
        r.archive_reason = Some(ArchiveReason::User);
        r.archive_changed_at = at_ms;
        r
    }

    fn run(
        prev: Option<&LiveSet>,
        current: &LiveSet,
        reachable: bool,
        store: &Store,
        probe: &FakeProbe,
    ) -> Vec<Mutation> {
        track(
            &Tick {
                prev,
                current,
                multiplexer_reachable: reachable,
                auto_track: true,
                now_ms: NOW,
            },
            store,
            probe,
            &mut Closing::default(),
            &HashSet::new(),
        )
    }

    fn run_with_created(current: &LiveSet, store: &Store, created: &[&str]) -> Vec<Mutation> {
        let created: HashSet<String> = created.iter().map(|pane| pane.to_string()).collect();
        track(
            &Tick {
                prev: None,
                current,
                multiplexer_reachable: true,
                auto_track: true,
                now_ms: NOW,
            },
            store,
            &FakeProbe::default(),
            &mut Closing::default(),
            &created,
        )
    }

    fn run_untracked(
        prev: Option<&LiveSet>,
        current: &LiveSet,
        store: &Store,
        probe: &FakeProbe,
        created: &[&str],
    ) -> Vec<Mutation> {
        let created: HashSet<String> = created.iter().map(|pane| pane.to_string()).collect();
        track(
            &Tick {
                prev,
                current,
                multiplexer_reachable: true,
                auto_track: false,
                now_ms: NOW,
            },
            store,
            probe,
            &mut Closing::default(),
            &created,
        )
    }

    // -- collapse --

    #[test]
    fn collapse_prefers_busy_pane() {
        let collapsed = set(vec![
            live("s", 1, LiveStatus::Idle),
            live("s", 2, LiveStatus::Busy),
            live("s", 3, LiveStatus::Idle),
        ]);
        assert_eq!(collapsed.values().count(), 1);
        assert_eq!(collapsed.get("s").unwrap().pid, 2);
    }

    #[test]
    fn collapse_without_busy_prefers_latest_start() {
        let mut older = live("s", 1, LiveStatus::Idle);
        older.started_at = Some(100_000);
        let mut newer = live("s", 2, LiveStatus::Idle);
        newer.started_at = Some(200_000);
        assert_eq!(
            set(vec![newer.clone(), older.clone()])
                .get("s")
                .unwrap()
                .pid,
            2
        );
        assert_eq!(set(vec![older, newer]).get("s").unwrap().pid, 2);
    }

    #[test]
    fn collapse_keeps_distinct_sessions() {
        let collapsed = set(vec![
            live("a", 1, LiveStatus::Idle),
            live("b", 2, LiveStatus::Idle),
        ]);
        assert_eq!(collapsed.values().count(), 2);
    }

    // -- new --

    #[test]
    fn unknown_live_session_is_inserted_as_detected() {
        let current = set(vec![live("s", 1, LiveStatus::Idle)]);
        let mutations = run(
            None,
            &current,
            true,
            &store_with(vec![]),
            &FakeProbe::default(),
        );

        let [Mutation::Insert(inserted)] = mutations.as_slice() else {
            panic!("expected one insert, got {mutations:?}");
        };
        assert_eq!(inserted.session_id, "s");
        assert_eq!(inserted.origin, Origin::Detected);
        assert_eq!(inserted.title, "name s");
        assert_eq!(inserted.cwd, "/work/app");
        assert_eq!(inserted.created_at, NOW / 1000);
        assert_eq!(inserted.archive_changed_at, NOW);
        assert_eq!(inserted.unread_changed_at, NOW);
        assert_eq!(inserted.last_activity, 900);
    }

    #[test]
    fn new_session_on_a_pane_we_spawned_is_inserted_as_created() {
        let current = set(vec![live("s", 1, LiveStatus::Idle)]);
        let mutations = run_with_created(&current, &store_with(vec![]), &["%1"]);
        let [Mutation::Insert(inserted)] = mutations.as_slice() else {
            panic!("expected one insert, got {mutations:?}");
        };
        assert_eq!(inserted.origin, Origin::Created);

        let mutations = run_with_created(&current, &store_with(vec![]), &["%9"]);
        let [Mutation::Insert(inserted)] = mutations.as_slice() else {
            panic!("expected one insert, got {mutations:?}");
        };
        assert_eq!(inserted.origin, Origin::Detected);
    }

    #[test]
    fn auto_track_off_skips_detected_sessions() {
        let current = set(vec![live("s", 1, LiveStatus::Idle)]);
        let mutations = run_untracked(
            None,
            &current,
            &store_with(vec![]),
            &FakeProbe::default(),
            &[],
        );
        assert!(mutations.is_empty(), "{mutations:?}");
    }

    #[test]
    fn auto_track_off_still_inserts_sessions_created_with_c() {
        let current = set(vec![
            live("made", 1, LiveStatus::Idle),
            live("other", 2, LiveStatus::Idle),
        ]);
        let mutations = run_untracked(
            None,
            &current,
            &store_with(vec![]),
            &FakeProbe::default(),
            &["%1"],
        );
        let [Mutation::Insert(inserted)] = mutations.as_slice() else {
            panic!("expected one insert, got {mutations:?}");
        };
        assert_eq!(inserted.session_id, "made");
        assert_eq!(inserted.origin, Origin::Created);
    }

    #[test]
    fn auto_track_off_keeps_tracking_stored_sessions() {
        let busy = set(vec![live("a", 1, LiveStatus::Busy)]);
        let idle = set(vec![live("a", 1, LiveStatus::Idle)]);
        let mut resumed = live("b", 2, LiveStatus::Idle);
        resumed.started_at = Some(500_000);
        let store = store_with(vec![record("a", 1), archived("b", 100_000)]);

        let unread = run_untracked(Some(&busy), &idle, &store, &FakeProbe::default(), &[]);
        assert_eq!(
            unread,
            vec![Mutation::SetUnread {
                session_id: "a".to_string(),
                unread: true
            }]
        );

        let unarchive = run_untracked(
            None,
            &set(vec![resumed]),
            &store,
            &FakeProbe::default(),
            &[],
        );
        assert_eq!(
            unarchive,
            vec![Mutation::Unarchive {
                session_id: "b".to_string()
            }]
        );

        let closed = run_untracked(
            Some(&idle),
            &LiveSet::default(),
            &store,
            &FakeProbe::default().with_transcript("a"),
            &[],
        );
        assert_eq!(
            closed,
            vec![Mutation::Archive {
                session_id: "a".to_string(),
                reason: ArchiveReason::Closed
            }]
        );
    }

    #[test]
    fn known_session_on_a_spawned_pane_is_not_reinserted() {
        let current = set(vec![live("s", 1, LiveStatus::Idle)]);
        let store = store_with(vec![record("s", 1)]);
        assert!(run_with_created(&current, &store, &["%1"]).is_empty());
    }

    #[test]
    fn new_session_without_transcript_activity_starts_at_now() {
        let mut session = live("s", 1, LiveStatus::Idle);
        session.last_activity = None;
        session.name = None;
        let mutations = run(
            None,
            &set(vec![session]),
            true,
            &store_with(vec![]),
            &FakeProbe::default(),
        );
        let [Mutation::Insert(inserted)] = mutations.as_slice() else {
            panic!("expected one insert");
        };
        assert_eq!(inserted.last_activity, NOW / 1000);
        assert_eq!(inserted.title, "");
    }

    #[test]
    fn known_live_session_is_not_inserted() {
        let current = set(vec![live("s", 1, LiveStatus::Idle)]);
        let store = store_with(vec![record("s", 1)]);
        assert!(run(
            Some(&current),
            &current,
            true,
            &store,
            &FakeProbe::default()
        )
        .is_empty());
    }

    // -- resumed --

    fn unarchive(id: &str) -> Mutation {
        Mutation::Unarchive {
            session_id: id.to_string(),
        }
    }

    #[test]
    fn process_started_after_the_archive_unarchives() {
        let mut resumed = live("s", 1, LiveStatus::Idle);
        resumed.started_at = Some(900_000);
        let current = set(vec![resumed]);
        let store = store_with(vec![archived("s", 800_000)]);

        // Whether we watched it appear or found it on the first tick makes no difference.
        assert_eq!(
            run(
                Some(&set(vec![])),
                &current,
                true,
                &store,
                &FakeProbe::default()
            ),
            vec![unarchive("s")]
        );
        assert_eq!(
            run(None, &current, true, &store, &FakeProbe::default()),
            vec![unarchive("s")]
        );
    }

    #[test]
    fn one_tick_discovery_blip_does_not_unarchive() {
        let mut session = live("s", 1, LiveStatus::Idle);
        session.started_at = Some(500_000);
        let current = set(vec![session]);
        let store = store_with(vec![archived("s", 800_000)]);

        // Absent from the previous tick, back now, but the process is the same old one.
        assert!(run(
            Some(&set(vec![])),
            &current,
            true,
            &store,
            &FakeProbe::default()
        )
        .is_empty());
    }

    #[test]
    fn process_started_exactly_at_the_archive_does_not_unarchive() {
        let mut session = live("s", 1, LiveStatus::Idle);
        session.started_at = Some(800_000);
        let store = store_with(vec![archived("s", 800_000)]);
        assert!(run(
            None,
            &set(vec![session]),
            true,
            &store,
            &FakeProbe::default()
        )
        .is_empty());
    }

    #[test]
    fn archived_live_session_stays_archived_while_continuously_live() {
        let mut session = live("s", 1, LiveStatus::Idle);
        session.started_at = Some(500_000);
        let current = set(vec![session]);
        let store = store_with(vec![archived("s", 800_000)]);

        assert!(run(
            Some(&current),
            &current,
            true,
            &store,
            &FakeProbe::default()
        )
        .is_empty());
        assert!(run(None, &current, true, &store, &FakeProbe::default()).is_empty());
    }

    #[test]
    fn archived_live_session_with_unknown_start_stays_archived() {
        let mut session = live("s", 1, LiveStatus::Idle);
        session.started_at = None;
        let store = store_with(vec![archived("s", 800_000)]);
        let current = set(vec![session]);
        assert!(run(None, &current, true, &store, &FakeProbe::default()).is_empty());
        assert!(run(
            Some(&set(vec![])),
            &current,
            true,
            &store,
            &FakeProbe::default()
        )
        .is_empty());
    }

    // -- busy -> idle --

    #[test]
    fn busy_to_idle_marks_unread() {
        let prev = set(vec![live("s", 1, LiveStatus::Busy)]);
        let current = set(vec![live("s", 1, LiveStatus::Idle)]);
        let store = store_with(vec![record("s", 1)]);
        assert_eq!(
            run(Some(&prev), &current, true, &store, &FakeProbe::default()),
            vec![Mutation::SetUnread {
                session_id: "s".to_string(),
                unread: true
            }]
        );
    }

    #[test]
    fn busy_to_idle_on_an_archived_session_does_not_mark_unread() {
        let prev = set(vec![live("s", 1, LiveStatus::Busy)]);
        let current = set(vec![live("s", 1, LiveStatus::Idle)]);
        let store = store_with(vec![archived("s", 800_000)]);
        assert!(run(Some(&prev), &current, true, &store, &FakeProbe::default()).is_empty());
    }

    #[test]
    fn other_status_pairs_do_not_mark_unread() {
        let store = store_with(vec![record("s", 1)]);
        for (before, after) in [
            (LiveStatus::Idle, LiveStatus::Idle),
            (LiveStatus::Idle, LiveStatus::Busy),
            (LiveStatus::Busy, LiveStatus::Busy),
        ] {
            let prev = set(vec![live("s", 1, before)]);
            let current = set(vec![live("s", 1, after)]);
            assert!(run(Some(&prev), &current, true, &store, &FakeProbe::default()).is_empty());
        }
    }

    #[test]
    fn busy_to_idle_on_first_tick_does_not_mark_unread() {
        let current = set(vec![live("s", 1, LiveStatus::Idle)]);
        let store = store_with(vec![record("s", 1)]);
        assert!(run(None, &current, true, &store, &FakeProbe::default()).is_empty());
    }

    #[test]
    fn already_unread_session_is_not_marked_again() {
        let mut unread = record("s", 1);
        unread.unread = true;
        let prev = set(vec![live("s", 1, LiveStatus::Busy)]);
        let current = set(vec![live("s", 1, LiveStatus::Idle)]);
        assert!(run(
            Some(&prev),
            &current,
            true,
            &store_with(vec![unread]),
            &FakeProbe::default()
        )
        .is_empty());
    }

    // -- closed --

    fn closed_setup() -> (LiveSet, LiveSet, Store) {
        (
            set(vec![live("s", 7, LiveStatus::Idle)]),
            set(vec![]),
            store_with(vec![record("s", 1)]),
        )
    }

    #[test]
    fn close_with_pid_file_gone_archives() {
        let (prev, current, store) = closed_setup();
        let probe = FakeProbe::default().with_transcript("s");
        assert_eq!(
            run(Some(&prev), &current, true, &store, &probe),
            vec![Mutation::Archive {
                session_id: "s".to_string(),
                reason: ArchiveReason::Closed,
            }]
        );
    }

    #[test]
    fn close_with_pid_file_still_present_is_a_crash_and_leaves_the_record() {
        let (prev, current, store) = closed_setup();
        let probe = FakeProbe::default()
            .with_transcript("s")
            .with_pid_file(7, "s");
        assert!(run(Some(&prev), &current, true, &store, &probe).is_empty());
    }

    #[test]
    fn close_with_pid_reused_for_a_different_session_archives() {
        let (prev, current, store) = closed_setup();
        let probe = FakeProbe::default()
            .with_transcript("s")
            .with_pid_file(7, "other");
        assert_eq!(
            run(Some(&prev), &current, true, &store, &probe),
            vec![Mutation::Archive {
                session_id: "s".to_string(),
                reason: ArchiveReason::Closed,
            }]
        );
    }

    #[test]
    fn clear_inside_the_same_pane_archives_old_and_inserts_new() {
        let prev = set(vec![live("old", 7, LiveStatus::Idle)]);
        let current = set(vec![live("new", 7, LiveStatus::Idle)]);
        let store = store_with(vec![record("old", 1)]);
        let probe = FakeProbe::default()
            .with_transcript("old")
            .with_pid_file(7, "new");
        let mutations = run(Some(&prev), &current, true, &store, &probe);
        assert_eq!(mutations.len(), 2);
        assert!(mutations
            .iter()
            .any(|m| matches!(m, Mutation::Insert(r) if r.session_id == "new")));
        assert!(mutations.contains(&Mutation::Archive {
            session_id: "old".to_string(),
            reason: ArchiveReason::Closed,
        }));
    }

    #[test]
    fn close_without_transcript_deletes_instead_of_archiving() {
        let (prev, current, store) = closed_setup();
        assert_eq!(
            run(Some(&prev), &current, true, &store, &FakeProbe::default()),
            vec![Mutation::Delete {
                session_id: "s".to_string()
            }]
        );
        let different_id = FakeProbe::default().with_pid_file(7, "other");
        assert_eq!(
            run(Some(&prev), &current, true, &store, &different_id),
            vec![Mutation::Delete {
                session_id: "s".to_string()
            }]
        );
    }

    #[test]
    fn close_with_unreadable_pid_file_is_pending_not_archived() {
        let (prev, current, store) = closed_setup();
        let mut closing = Closing::default();
        let malformed = FakeProbe::default()
            .with_transcript("s")
            .with_unreadable_pid_file(7);
        assert!(tick(&prev, &current, &store, &malformed, &mut closing, 100).is_empty());
        assert!(tick(&current, &current, &store, &malformed, &mut closing, 102).is_empty());

        // It is re-examined, and a clean file removal still archives.
        let gone = FakeProbe::default().with_transcript("s");
        assert_eq!(
            tick(&current, &current, &store, &gone, &mut closing, 104),
            vec![archive("s")]
        );
    }

    #[test]
    fn unreadable_pid_file_never_deletes_a_session_without_transcript() {
        let (prev, current, store) = closed_setup();
        let malformed = FakeProbe::default().with_unreadable_pid_file(7);
        assert!(run(Some(&prev), &current, true, &store, &malformed).is_empty());
    }

    #[test]
    fn unreadable_pid_file_gives_up_after_the_grace_period() {
        let (prev, current, store) = closed_setup();
        let mut closing = Closing::default();
        let malformed = FakeProbe::default()
            .with_transcript("s")
            .with_unreadable_pid_file(7);
        tick(&prev, &current, &store, &malformed, &mut closing, 100);
        assert!(tick(&current, &current, &store, &malformed, &mut closing, 110).is_empty());
        let gone = FakeProbe::default().with_transcript("s");
        assert!(tick(&current, &current, &store, &gone, &mut closing, 112).is_empty());
    }

    #[test]
    fn crash_without_transcript_is_still_left_alone() {
        let (prev, current, store) = closed_setup();
        let probe = FakeProbe::default().with_pid_file(7, "s");
        assert!(run(Some(&prev), &current, true, &store, &probe).is_empty());
    }

    #[test]
    fn closing_an_already_archived_session_does_not_rearchive_it() {
        let prev = set(vec![live("s", 7, LiveStatus::Idle)]);
        let current = set(vec![]);
        let store = store_with(vec![archived("s", 800)]);
        let probe = FakeProbe::default().with_transcript("s");
        assert!(run(Some(&prev), &current, true, &store, &probe).is_empty());
    }

    #[test]
    fn closing_an_archived_session_without_transcript_deletes_it() {
        let prev = set(vec![live("s", 7, LiveStatus::Idle)]);
        let current = set(vec![]);
        let store = store_with(vec![archived("s", 800)]);
        assert_eq!(
            run(Some(&prev), &current, true, &store, &FakeProbe::default()),
            vec![Mutation::Delete {
                session_id: "s".to_string()
            }]
        );
    }

    #[test]
    fn closing_an_unknown_session_does_nothing() {
        let (prev, current, _) = closed_setup();
        assert!(run(
            Some(&prev),
            &current,
            true,
            &store_with(vec![]),
            &FakeProbe::default()
        )
        .is_empty());
    }

    #[test]
    fn session_still_live_in_another_pane_is_not_closed() {
        let prev = set(vec![
            live("s", 7, LiveStatus::Busy),
            live("s", 8, LiveStatus::Idle),
        ]);
        let current = set(vec![live("s", 8, LiveStatus::Idle)]);
        let store = store_with(vec![record("s", 1)]);
        let mutations = run(Some(&prev), &current, true, &store, &FakeProbe::default());
        assert!(!mutations
            .iter()
            .any(|m| matches!(m, Mutation::Archive { .. } | Mutation::Delete { .. })));
    }

    // -- pending close (pid file still there, process still alive) --

    fn archive(id: &str) -> Mutation {
        Mutation::Archive {
            session_id: id.to_string(),
            reason: ArchiveReason::Closed,
        }
    }

    /// Tick at `now_secs` with the given probe, sharing `closing` across calls.
    fn tick(
        prev: &LiveSet,
        current: &LiveSet,
        store: &Store,
        probe: &FakeProbe,
        closing: &mut Closing,
        now_secs: u64,
    ) -> Vec<Mutation> {
        track(
            &Tick {
                prev: Some(prev),
                current,
                multiplexer_reachable: true,
                auto_track: true,
                now_ms: now_secs * 1000,
            },
            store,
            probe,
            closing,
            &HashSet::new(),
        )
    }

    #[test]
    fn same_id_with_live_pid_waits_then_archives_when_the_file_goes() {
        let (prev, current, store) = closed_setup();
        let mut closing = Closing::default();
        let lingering = FakeProbe::default()
            .with_transcript("s")
            .with_pid_file(7, "s")
            .with_alive(7);
        assert!(tick(&prev, &current, &store, &lingering, &mut closing, 100).is_empty());

        // prev has moved on to the empty set by the next tick.
        let probe = FakeProbe::default().with_transcript("s").with_alive(7);
        assert!(tick(&current, &current, &store, &lingering, &mut closing, 102).is_empty());
        assert_eq!(
            tick(&current, &current, &store, &probe, &mut closing, 104),
            vec![archive("s")]
        );
        assert!(tick(&current, &current, &store, &probe, &mut closing, 106).is_empty());
    }

    #[test]
    fn pending_close_resolves_to_a_crash_when_the_process_dies() {
        let (prev, current, store) = closed_setup();
        let mut closing = Closing::default();
        let lingering = FakeProbe::default()
            .with_transcript("s")
            .with_pid_file(7, "s")
            .with_alive(7);
        tick(&prev, &current, &store, &lingering, &mut closing, 100);

        let dead = FakeProbe::default()
            .with_transcript("s")
            .with_pid_file(7, "s");
        assert!(tick(&current, &current, &store, &dead, &mut closing, 102).is_empty());
        // Resolved: nothing happens later even if the file disappears (sweep by another claude).
        assert!(tick(
            &current,
            &current,
            &store,
            &FakeProbe::default().with_transcript("s"),
            &mut closing,
            104
        )
        .is_empty());
    }

    #[test]
    fn pending_close_times_out_without_archiving() {
        let (prev, current, store) = closed_setup();
        let mut closing = Closing::default();
        let lingering = FakeProbe::default()
            .with_transcript("s")
            .with_pid_file(7, "s")
            .with_alive(7);
        assert!(tick(&prev, &current, &store, &lingering, &mut closing, 100).is_empty());
        assert!(tick(&current, &current, &store, &lingering, &mut closing, 109).is_empty());
        assert!(tick(&current, &current, &store, &lingering, &mut closing, 110).is_empty());

        // Given up: even a later file removal doesn't archive it.
        let gone = FakeProbe::default().with_transcript("s");
        assert!(tick(&current, &current, &store, &gone, &mut closing, 112).is_empty());
    }

    #[test]
    fn pending_close_is_dropped_when_the_session_reappears_live() {
        let (prev, current, store) = closed_setup();
        let mut closing = Closing::default();
        let lingering = FakeProbe::default()
            .with_transcript("s")
            .with_pid_file(7, "s")
            .with_alive(7);
        tick(&prev, &current, &store, &lingering, &mut closing, 100);

        let back = set(vec![live("s", 9, LiveStatus::Idle)]);
        assert!(tick(&current, &back, &store, &lingering, &mut closing, 102).is_empty());

        // Gone again for real: classified fresh from prev (pid 9, no file).
        let gone = FakeProbe::default().with_transcript("s");
        assert_eq!(
            tick(&back, &current, &store, &gone, &mut closing, 104),
            vec![archive("s")]
        );
    }

    #[test]
    fn pending_close_is_not_reexamined_while_unreachable() {
        let (prev, current, store) = closed_setup();
        let mut closing = Closing::default();
        let lingering = FakeProbe::default()
            .with_transcript("s")
            .with_pid_file(7, "s")
            .with_alive(7);
        tick(&prev, &current, &store, &lingering, &mut closing, 100);

        let gone = FakeProbe::default().with_transcript("s");
        let unreachable = Tick {
            prev: None,
            current: &current,
            multiplexer_reachable: false,
            auto_track: true,
            now_ms: 102_000,
        };
        assert!(track(&unreachable, &store, &gone, &mut closing, &HashSet::new()).is_empty());
        assert_eq!(
            tick(&current, &current, &store, &gone, &mut closing, 104),
            vec![archive("s")]
        );
    }

    #[test]
    fn pending_close_without_transcript_deletes_on_resolution() {
        let (prev, current, store) = closed_setup();
        let mut closing = Closing::default();
        let lingering = FakeProbe::default().with_pid_file(7, "s").with_alive(7);
        tick(&prev, &current, &store, &lingering, &mut closing, 100);
        assert_eq!(
            tick(
                &current,
                &current,
                &store,
                &FakeProbe::default(),
                &mut closing,
                102
            ),
            vec![Mutation::Delete {
                session_id: "s".to_string()
            }]
        );
    }

    // -- first tick / unreachable --

    #[test]
    fn first_tick_never_closes_sessions() {
        let store = store_with(vec![record("s", 1)]);
        let current = set(vec![]);
        assert!(run(None, &current, true, &store, &FakeProbe::default()).is_empty());
    }

    #[test]
    fn unreachable_multiplexer_never_closes_sessions() {
        let (prev, current, store) = closed_setup();
        let probe = FakeProbe::default().with_transcript("s");
        assert!(run(Some(&prev), &current, false, &store, &probe).is_empty());
    }

    // -- metadata refresh --

    #[test]
    fn refresh_emits_only_changed_fields() {
        let mut stored = record("s", 1);
        stored.title = "name s".to_string();
        stored.cwd = "/old".to_string();
        stored.last_activity = 900;
        let mutations = refresh_metadata(
            &set(vec![live("s", 1, LiveStatus::Idle)]),
            &store_with(vec![stored]),
        );
        assert_eq!(
            mutations,
            vec![Mutation::Refresh {
                session_id: "s".to_string(),
                title: None,
                cwd: Some("/work/app".to_string()),
                last_activity: None,
            }]
        );
    }

    #[test]
    fn refresh_replaces_a_stored_derived_name_with_the_live_title() {
        let mut stored = record("s", 1);
        stored.title = "stellar-ptarmigan-03".to_string();
        stored.cwd = "/work/app".to_string();
        stored.last_activity = 900;
        let mut session = live("s", 1, LiveStatus::Idle);
        session.name = Some("Fix the parser".to_string());
        assert_eq!(
            refresh_metadata(&set(vec![session]), &store_with(vec![stored])),
            vec![Mutation::Refresh {
                session_id: "s".to_string(),
                title: Some("Fix the parser".to_string()),
                cwd: None,
                last_activity: None,
            }]
        );
    }

    #[test]
    fn refresh_never_lowers_last_activity() {
        let mut stored = record("s", 1);
        stored.title = "name s".to_string();
        stored.cwd = "/work/app".to_string();
        stored.last_activity = 950;
        assert!(refresh_metadata(
            &set(vec![live("s", 1, LiveStatus::Idle)]),
            &store_with(vec![stored])
        )
        .is_empty());
    }

    #[test]
    fn refresh_is_silent_when_nothing_changed() {
        let mut stored = record("s", 1);
        stored.title = "name s".to_string();
        stored.cwd = "/work/app".to_string();
        stored.last_activity = 900;
        assert!(refresh_metadata(
            &set(vec![live("s", 1, LiveStatus::Idle)]),
            &store_with(vec![stored])
        )
        .is_empty());
    }

    #[test]
    fn refresh_keeps_stored_values_the_live_side_has_none_for() {
        let mut session = live("s", 1, LiveStatus::Idle);
        session.name = None;
        session.last_activity = None;
        let mut stored = record("s", 1);
        stored.cwd = "/work/app".to_string();
        assert!(refresh_metadata(&set(vec![session]), &store_with(vec![stored])).is_empty());
    }

    #[test]
    fn refresh_ignores_sessions_missing_from_the_store() {
        assert!(refresh_metadata(
            &set(vec![live("s", 1, LiveStatus::Idle)]),
            &store_with(vec![])
        )
        .is_empty());
    }

    // -- apply --

    #[test]
    fn apply_archive_records_the_reason_and_unarchive_clears_it() {
        let mut store = store_with(vec![record("s", 1), record("t", 1)]);
        let archive = |id: &str, reason| Mutation::Archive {
            session_id: id.to_string(),
            reason,
        };
        assert!(apply(
            &mut store,
            vec![
                archive("s", ArchiveReason::User),
                archive("t", ArchiveReason::Closed)
            ],
            50_000
        ));
        let user = store.get("s").unwrap();
        assert_eq!(user.archived_at, Some(50));
        assert_eq!(user.archive_reason, Some(ArchiveReason::User));
        assert_eq!(user.archive_changed_at, 50_000);
        assert_eq!(
            store.get("t").unwrap().archive_reason,
            Some(ArchiveReason::Closed)
        );

        assert!(apply(&mut store, vec![unarchive("s")], 60_000));
        let resumed = store.get("s").unwrap();
        assert_eq!(resumed.archived_at, None);
        assert_eq!(resumed.archive_reason, None);
        assert_eq!(resumed.archive_changed_at, 60_000);
    }

    #[test]
    fn apply_archive_clears_unread() {
        let mut unread = record("s", 1);
        unread.unread = true;
        let mut store = store_with(vec![unread]);
        apply(
            &mut store,
            vec![Mutation::Archive {
                session_id: "s".to_string(),
                reason: ArchiveReason::User,
            }],
            50_000,
        );
        let archived = store.get("s").unwrap();
        assert!(!archived.unread);
        assert_eq!(archived.unread_changed_at, 50_000);
    }

    #[test]
    fn apply_unread_sets_and_clears_the_flag() {
        let mut store = store_with(vec![record("a", 1)]);
        let unread = |unread| Mutation::SetUnread {
            session_id: "a".to_string(),
            unread,
        };
        apply(&mut store, vec![unread(true)], 5);
        assert!(store.get("a").unwrap().unread);
        assert_eq!(store.get("a").unwrap().unread_changed_at, 5);

        apply(&mut store, vec![unread(false)], 6);
        assert!(!store.get("a").unwrap().unread);
        assert_eq!(store.get("a").unwrap().unread_changed_at, 6);
    }

    #[test]
    fn apply_delete_and_insert_report_changes() {
        let mut store = store_with(vec![record("a", 1)]);
        assert!(apply(
            &mut store,
            vec![Mutation::Delete {
                session_id: "a".to_string()
            }],
            5
        ));
        assert!(store.get("a").is_none());
        assert!(apply(&mut store, vec![Mutation::Insert(record("b", 5))], 5));
        assert!(!apply(
            &mut store,
            vec![Mutation::Insert(record("b", 9))],
            9
        ));
        assert_eq!(store.get("b").unwrap().archive_changed_at, 5);
    }

    #[test]
    fn apply_on_unknown_session_changes_nothing() {
        let mut store = store_with(vec![]);
        assert!(!apply(
            &mut store,
            vec![Mutation::Archive {
                session_id: "ghost".to_string(),
                reason: ArchiveReason::Closed,
            }],
            5
        ));
    }

    #[test]
    fn apply_refresh_updates_only_given_fields() {
        let mut store = store_with(vec![record("s", 1)]);
        apply(
            &mut store,
            vec![Mutation::Refresh {
                session_id: "s".to_string(),
                title: Some("new".to_string()),
                cwd: None,
                last_activity: Some(77),
            }],
            9,
        );
        let updated = store.get("s").unwrap();
        assert_eq!(updated.title, "new");
        assert_eq!(updated.cwd, "/work/app");
        assert_eq!(updated.last_activity, 77);
    }

    // -- prompt state --

    #[test]
    fn prompt_state_is_read_from_the_last_non_empty_line() {
        assert_eq!(
            detect_prompt_state("body\n  ctrl-g to edit in vim\n\n"),
            PromptState::Plan
        );
        assert_eq!(
            detect_prompt_state("x\nEnter to select\n"),
            PromptState::Ask
        );
        assert_eq!(
            detect_prompt_state("ctrl-g to edit\nplain"),
            PromptState::None
        );
        assert_eq!(detect_prompt_state(""), PromptState::None);
    }
}
