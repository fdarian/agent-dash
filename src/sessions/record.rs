use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Detected,
    Created,
    ImportedCli,
    ImportedDesktop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveReason {
    /// Archived with `a`.
    User,
    /// Archived because the session's process went away.
    Closed,
}

/// Deleted-session tombstones older than this are dropped on save.
const TOMBSTONE_TTL_MS: u64 = 30 * 24 * 60 * 60 * 1000;

/// A Claude session that agent-dash knows about, live or not.
///
/// Timestamps ending in `_changed_at` are epoch milliseconds; the other timestamps are seconds.
/// Three groups of fields merge independently when several instances save the same session:
/// archive state (`archived_at`, `archive_reason`, `archive_changed_at`), unread state
/// (`unread`, `unread_changed_at`), and metadata (`title`, `cwd`, `last_activity`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", from = "RawRecord")]
pub struct SessionRecord {
    pub session_id: String,
    pub cwd: String,
    /// Empty when the session has no name yet.
    pub title: String,
    pub created_at: u64,
    pub last_activity: u64,
    pub archived_at: Option<u64>,
    /// `None` when not archived, and for records archived before the reason was recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive_reason: Option<ArchiveReason>,
    pub archive_changed_at: u64,
    pub unread: bool,
    pub unread_changed_at: u64,
    pub origin: Origin,
}

impl SessionRecord {
    /// A freshly inserted record: inserting is the initial archive and unread intent.
    pub fn new(
        session_id: String,
        cwd: String,
        title: String,
        created_at: u64,
        last_activity: u64,
        origin: Origin,
        now_ms: u64,
    ) -> SessionRecord {
        SessionRecord {
            session_id,
            cwd,
            title,
            created_at,
            last_activity,
            archived_at: None,
            archive_reason: None,
            archive_changed_at: now_ms,
            unread: false,
            unread_changed_at: now_ms,
            origin,
        }
    }

    fn last_intent_change(&self) -> u64 {
        self.archive_changed_at.max(self.unread_changed_at)
    }
}

/// The on-disk shape, which also reads files written before per-field timestamps existed.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRecord {
    session_id: String,
    cwd: String,
    title: String,
    created_at: u64,
    last_activity: u64,
    #[serde(default)]
    archived_at: Option<u64>,
    #[serde(default)]
    archive_reason: Option<ArchiveReason>,
    #[serde(default)]
    archive_changed_at: Option<u64>,
    #[serde(default)]
    unread: bool,
    #[serde(default)]
    unread_changed_at: Option<u64>,
    origin: Origin,
}

impl From<RawRecord> for SessionRecord {
    fn from(raw: RawRecord) -> SessionRecord {
        // Old files only know when a session was archived and created, so those stand in for
        // the missing change times; the old whole-record `updated_at` is deliberately ignored.
        let inserted_ms = raw.created_at * 1000;
        SessionRecord {
            archive_changed_at: raw
                .archive_changed_at
                .or(raw.archived_at.map(|archived| archived * 1000))
                .unwrap_or(inserted_ms),
            unread_changed_at: raw.unread_changed_at.unwrap_or(inserted_ms),
            session_id: raw.session_id,
            cwd: raw.cwd,
            title: raw.title,
            created_at: raw.created_at,
            last_activity: raw.last_activity,
            archived_at: raw.archived_at,
            archive_reason: raw.archive_reason,
            unread: raw.unread,
            origin: raw.origin,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct StoreFile {
    sessions: Vec<SessionRecord>,
    /// Session id -> epoch ms it was deleted.
    #[serde(default)]
    deleted: BTreeMap<String, u64>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct StoreData {
    records: BTreeMap<String, SessionRecord>,
    deleted: BTreeMap<String, u64>,
}

/// What the file looked like when we last read or wrote it, to skip re-reading an unchanged one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    modified: SystemTime,
    len: u64,
}

#[derive(Debug, Default, Clone, Copy)]
struct Unsaved {
    /// Archive, unread, insert or delete: must reach disk right away.
    intent: bool,
    /// Title, cwd or last activity: can wait.
    metadata: bool,
}

/// Records persisted in `sessions.json`, shared by every running agent-dash instance.
#[derive(Debug)]
pub struct Store {
    path: PathBuf,
    data: StoreData,
    unsaved: Unsaved,
    seen: Option<FileStamp>,
}

pub fn default_store_path() -> PathBuf {
    crate::config::config_dir().join("sessions.json")
}

impl Store {
    /// `None` when the file doesn't exist yet (first run).
    pub fn load(path: PathBuf) -> Result<Option<Store>> {
        let seen = file_stamp(&path)?;
        let Some(data) = read_file(&path)? else {
            return Ok(None);
        };
        Ok(Some(Store {
            path,
            data,
            unsaved: Unsaved::default(),
            seen,
        }))
    }

    pub fn empty(path: PathBuf) -> Store {
        Store {
            path,
            data: StoreData::default(),
            unsaved: Unsaved::default(),
            seen: None,
        }
    }

    pub fn get(&self, session_id: &str) -> Option<&SessionRecord> {
        self.data.records.get(session_id)
    }

    pub fn records(&self) -> impl Iterator<Item = &SessionRecord> {
        self.data.records.values()
    }

    #[cfg(test)]
    pub fn has_tombstone(&self, session_id: &str) -> bool {
        self.data.deleted.contains_key(session_id)
    }

    pub fn has_unsaved_intent(&self) -> bool {
        self.unsaved.intent
    }

    pub fn has_unsaved_metadata(&self) -> bool {
        self.unsaved.metadata
    }

    pub fn insert(&mut self, record: SessionRecord) {
        self.data.deleted.remove(&record.session_id);
        self.data.records.insert(record.session_id.clone(), record);
        self.unsaved.intent = true;
    }

    /// Archiving also clears unread, so archived rows never show the unread mark.
    /// Returns whether anything changed.
    pub fn archive(&mut self, session_id: &str, reason: ArchiveReason, now_ms: u64) -> bool {
        let Some(record) = self.data.records.get_mut(session_id) else {
            return false;
        };
        if record.archived_at.is_some() {
            return false;
        }
        record.archived_at = Some(now_ms / 1000);
        record.archive_reason = Some(reason);
        record.archive_changed_at = now_ms;
        if record.unread {
            record.unread = false;
            record.unread_changed_at = now_ms;
        }
        self.unsaved.intent = true;
        true
    }

    pub fn unarchive(&mut self, session_id: &str, now_ms: u64) -> bool {
        let Some(record) = self.data.records.get_mut(session_id) else {
            return false;
        };
        if record.archived_at.is_none() {
            return false;
        }
        record.archived_at = None;
        record.archive_reason = None;
        record.archive_changed_at = now_ms;
        self.unsaved.intent = true;
        true
    }

    pub fn set_unread(&mut self, session_id: &str, unread: bool, now_ms: u64) -> bool {
        let Some(record) = self.data.records.get_mut(session_id) else {
            return false;
        };
        if record.unread == unread {
            return false;
        }
        record.unread = unread;
        record.unread_changed_at = now_ms;
        self.unsaved.intent = true;
        true
    }

    /// Returns whether anything changed.
    pub fn refresh(
        &mut self,
        session_id: &str,
        title: Option<String>,
        cwd: Option<String>,
        last_activity: Option<u64>,
    ) -> bool {
        let Some(record) = self.data.records.get_mut(session_id) else {
            return false;
        };
        let mut changed = false;
        if let Some(title) = title.filter(|title| *title != record.title) {
            record.title = title;
            changed = true;
        }
        if let Some(cwd) = cwd.filter(|cwd| *cwd != record.cwd) {
            record.cwd = cwd;
            changed = true;
        }
        if let Some(last_activity) = last_activity.filter(|at| *at != record.last_activity) {
            record.last_activity = last_activity;
            changed = true;
        }
        self.unsaved.metadata |= changed;
        changed
    }

    pub fn remove(&mut self, session_id: &str, now_ms: u64) -> bool {
        if self.data.records.remove(session_id).is_none() {
            return false;
        }
        self.data.deleted.insert(session_id.to_string(), now_ms);
        self.unsaved.intent = true;
        true
    }

    /// Merges what other instances wrote, then writes the result and adopts it. The unsaved
    /// flags stay set when the write fails, so the caller can retry.
    pub fn save(&mut self, now_ms: u64) -> Result<()> {
        self.merge_from_disk()?;
        self.data
            .deleted
            .retain(|_, deleted_at| now_ms.saturating_sub(*deleted_at) < TOMBSTONE_TTL_MS);
        write_atomic(&self.path, &self.data)?;
        self.unsaved = Unsaved::default();
        self.seen = file_stamp(&self.path)?;
        Ok(())
    }

    /// Does nothing while the file is as we last saw it.
    pub fn sync(&mut self) -> Result<()> {
        let stamp = file_stamp(&self.path)?;
        if stamp == self.seen {
            return Ok(());
        }
        self.merge_from_disk()
    }

    fn merge_from_disk(&mut self) -> Result<()> {
        // Recorded before reading so a file that fails to parse is reported once per change
        // instead of on every tick.
        self.seen = file_stamp(&self.path)?;
        let on_disk = read_file(&self.path)?.unwrap_or_default();
        self.data = merge(on_disk, &self.data);
        Ok(())
    }
}

/// Per-field merge of two copies of the store. Deletions are tombstones: a record is dropped
/// unless one of its intent fields changed after the session was deleted, which is how a
/// session that became live again comes back.
fn merge(on_disk: StoreData, mine: &StoreData) -> StoreData {
    let mut deleted = on_disk.deleted;
    for (id, deleted_at) in &mine.deleted {
        let entry = deleted.entry(id.clone()).or_insert(*deleted_at);
        *entry = (*entry).max(*deleted_at);
    }

    let mut records = on_disk.records;
    for (id, record) in &mine.records {
        let merged = match records.remove(id) {
            Some(disk_record) => merge_record(disk_record, record),
            None => record.clone(),
        };
        records.insert(id.clone(), merged);
    }
    records.retain(|id, record| {
        deleted
            .get(id)
            .is_none_or(|deleted_at| record.last_intent_change() > *deleted_at)
    });

    StoreData { records, deleted }
}

/// Newer `*_changed_at` wins per intent group, the larger `last_activity` wins metadata.
/// Ties go to `mine`.
fn merge_record(disk: SessionRecord, mine: &SessionRecord) -> SessionRecord {
    let mut merged = if disk.last_activity > mine.last_activity {
        disk.clone()
    } else {
        mine.clone()
    };
    let archive = if disk.archive_changed_at > mine.archive_changed_at {
        &disk
    } else {
        mine
    };
    merged.archived_at = archive.archived_at;
    merged.archive_reason = archive.archive_reason;
    merged.archive_changed_at = archive.archive_changed_at;
    let unread = if disk.unread_changed_at > mine.unread_changed_at {
        &disk
    } else {
        mine
    };
    merged.unread = unread.unread;
    merged.unread_changed_at = unread.unread_changed_at;
    merged
}

fn file_stamp(path: &Path) -> Result<Option<FileStamp>> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(Some(FileStamp {
            modified: metadata
                .modified()
                .with_context(|| format!("failed to read mtime of {}", path.display()))?,
            len: metadata.len(),
        })),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("failed to stat {}", path.display())),
    }
}

fn read_file(path: &Path) -> Result<Option<StoreData>> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).with_context(|| format!("failed to read {}", path.display())),
    };
    let file: StoreFile = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(StoreData {
        records: file
            .sessions
            .into_iter()
            .map(|record| (record.session_id.clone(), record))
            .collect(),
        deleted: file.deleted,
    }))
}

fn write_atomic(path: &Path, data: &StoreData) -> Result<()> {
    let dir = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    std::fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let file = StoreFile {
        sessions: data.records.values().cloned().collect(),
        deleted: data.deleted.clone(),
    };
    let tmp = dir.join(format!(".sessions.json.{}.tmp", std::process::id()));
    std::fs::write(&tmp, serde_json::to_string(&file)?)
        .with_context(|| format!("failed to write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Both intent timestamps are `changed_at` (ms).
    pub fn record(id: &str, changed_at: u64) -> SessionRecord {
        SessionRecord {
            session_id: id.to_string(),
            cwd: "/work/app".to_string(),
            title: format!("title {id}"),
            created_at: 1,
            last_activity: 1,
            archived_at: None,
            archive_reason: None,
            archive_changed_at: changed_at,
            unread: false,
            unread_changed_at: changed_at,
            origin: Origin::Detected,
        }
    }

    pub struct TempDir(pub PathBuf);

    impl TempDir {
        pub fn new(name: &str) -> TempDir {
            let dir = std::env::temp_dir().join(format!(
                "agent-dash-test-{}-{}-{}",
                name,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        pub fn store_path(&self) -> PathBuf {
            self.0.join("sessions.json")
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn data(records: Vec<SessionRecord>, deleted: &[(&str, u64)]) -> StoreData {
        StoreData {
            records: records
                .into_iter()
                .map(|r| (r.session_id.clone(), r))
                .collect(),
            deleted: deleted
                .iter()
                .map(|(id, at)| (id.to_string(), *at))
                .collect(),
        }
    }

    fn archived(mut r: SessionRecord, at_ms: u64, reason: ArchiveReason) -> SessionRecord {
        r.archived_at = Some(at_ms / 1000);
        r.archive_reason = Some(reason);
        r.archive_changed_at = at_ms;
        r
    }

    // -- per-field merge --

    #[test]
    fn merge_archive_state_is_independent_of_unread_and_metadata() {
        let mut disk = archived(record("a", 10), 50, ArchiveReason::User);
        disk.title = "disk title".to_string();
        disk.last_activity = 100;
        let mut mine = record("a", 10);
        mine.unread = true;
        mine.unread_changed_at = 60;
        mine.last_activity = 5;

        let merged = merge(data(vec![disk], &[]), &data(vec![mine], &[]));
        let merged = &merged.records["a"];
        assert_eq!(merged.archived_at, Some(0));
        assert_eq!(merged.archive_reason, Some(ArchiveReason::User));
        assert_eq!(merged.archive_changed_at, 50);
        assert!(merged.unread);
        assert_eq!(merged.unread_changed_at, 60);
        assert_eq!(merged.title, "disk title");
        assert_eq!(merged.last_activity, 100);
    }

    #[test]
    fn merge_newer_unarchive_beats_older_archive() {
        let disk = archived(record("a", 10), 50, ArchiveReason::Closed);
        let mut mine = record("a", 10);
        mine.archive_changed_at = 70;

        let merged = merge(data(vec![disk], &[]), &data(vec![mine], &[]));
        assert_eq!(merged.records["a"].archived_at, None);
        assert_eq!(merged.records["a"].archive_reason, None);
    }

    #[test]
    fn merge_newer_unread_change_wins_without_touching_archive() {
        let mut disk = record("a", 10);
        disk.unread = true;
        disk.unread_changed_at = 90;
        let mine = archived(record("a", 10), 80, ArchiveReason::User);

        let merged = merge(data(vec![disk], &[]), &data(vec![mine], &[]));
        let merged = &merged.records["a"];
        assert!(merged.unread);
        assert!(merged.archived_at.is_some());
    }

    #[test]
    fn merge_metadata_follows_the_larger_last_activity_and_never_touches_intent() {
        let mut disk = record("a", 10);
        disk.title = "disk".to_string();
        disk.cwd = "/disk".to_string();
        disk.last_activity = 20;
        let mut mine = record("a", 99);
        mine.title = "mine".to_string();
        mine.last_activity = 30;
        mine.unread = true;

        let merged = merge(
            data(vec![disk.clone()], &[]),
            &data(vec![mine.clone()], &[]),
        );
        assert_eq!(merged.records["a"].title, "mine");
        assert_eq!(merged.records["a"].cwd, "/work/app");
        assert!(merged.records["a"].unread);

        mine.last_activity = 10;
        let merged = merge(data(vec![disk], &[]), &data(vec![mine], &[]));
        assert_eq!(merged.records["a"].title, "disk");
        assert_eq!(merged.records["a"].cwd, "/disk");
        // Intent still comes from the side with the newer timestamp, not the metadata winner.
        assert!(merged.records["a"].unread);
        assert_eq!(merged.records["a"].unread_changed_at, 99);
    }

    #[test]
    fn merge_ties_go_to_mine() {
        let mut disk = record("a", 10);
        disk.title = "disk".to_string();
        disk.unread = true;
        let mut mine = record("a", 10);
        mine.title = "mine".to_string();

        let merged = merge(data(vec![disk], &[]), &data(vec![mine], &[]));
        assert_eq!(merged.records["a"].title, "mine");
        assert!(!merged.records["a"].unread);
    }

    #[test]
    fn merge_keeps_records_only_one_side_has() {
        let merged = merge(
            data(vec![record("disk-only", 1)], &[]),
            &data(vec![record("mine-only", 1)], &[]),
        );
        assert_eq!(merged.records.len(), 2);
    }

    // -- tombstones --

    #[test]
    fn tombstone_drops_a_stale_record_from_either_side() {
        let merged = merge(
            data(vec![record("a", 10), record("b", 10)], &[]),
            &data(vec![record("c", 10)], &[("a", 10), ("c", 20)]),
        );
        assert!(!merged.records.contains_key("a"), "disk copy, deleted here");
        assert!(merged.records.contains_key("b"));
        assert!(
            !merged.records.contains_key("c"),
            "my stale copy, deleted on disk"
        );
        let merged = merge(
            data(vec![record("c", 10)], &[("c", 20)]),
            &data(vec![record("c", 10)], &[]),
        );
        assert!(merged.records.is_empty());
    }

    #[test]
    fn tombstone_keeps_the_later_deletion_time_from_either_side() {
        let merged = merge(
            data(vec![], &[("a", 10), ("b", 30)]),
            &data(vec![], &[("a", 20), ("b", 5)]),
        );
        assert_eq!(merged.deleted["a"], 20);
        assert_eq!(merged.deleted["b"], 30);
    }

    #[test]
    fn newer_insert_beats_a_tombstone() {
        let mut revived = record("a", 10);
        revived.unread_changed_at = 31;
        let merged = merge(data(vec![revived], &[("a", 30)]), &data(vec![], &[]));
        assert!(merged.records.contains_key("a"));

        // Equal is not newer.
        let merged = merge(
            data(vec![record("a", 30)], &[("a", 30)]),
            &data(vec![], &[]),
        );
        assert!(merged.records.is_empty());
    }

    #[test]
    fn reinserting_after_a_delete_survives_the_next_save() {
        let dir = TempDir::new("reinsert");
        let mut store = Store::empty(dir.store_path());
        store.insert(record("a", 1));
        store.save(10).unwrap();
        store.remove("a", 20);
        store.save(21).unwrap();
        assert!(Store::load(dir.store_path())
            .unwrap()
            .unwrap()
            .get("a")
            .is_none());

        store.insert(record("a", 30));
        store.save(31).unwrap();
        assert!(Store::load(dir.store_path())
            .unwrap()
            .unwrap()
            .get("a")
            .is_some());
    }

    #[test]
    fn tombstones_persist_across_load_and_stop_resurrection() {
        let dir = TempDir::new("tombstone-persist");
        let mut first = Store::empty(dir.store_path());
        first.insert(record("a", 1));
        first.save(2).unwrap();
        let mut stale = Store::load(dir.store_path()).unwrap().unwrap();

        first.remove("a", 20);
        first.save(21).unwrap();

        let loaded = Store::load(dir.store_path()).unwrap().unwrap();
        assert_eq!(loaded.data.deleted["a"], 20);
        // An instance that still holds the record saves it back: the tombstone beats it.
        stale.save(22).unwrap();
        assert!(stale.get("a").is_none());
        assert!(Store::load(dir.store_path())
            .unwrap()
            .unwrap()
            .get("a")
            .is_none());
    }

    #[test]
    fn tombstones_older_than_30_days_are_pruned_on_save() {
        let dir = TempDir::new("tombstone-prune");
        let mut store = Store::empty(dir.store_path());
        store.insert(record("old", 1));
        store.insert(record("recent", 1));
        store.remove("old", 1_000);
        store.remove("recent", 1_000 + TOMBSTONE_TTL_MS - 1);

        store.save(1_000 + TOMBSTONE_TTL_MS).unwrap();

        let loaded = Store::load(dir.store_path()).unwrap().unwrap();
        assert!(!loaded.data.deleted.contains_key("old"));
        assert!(loaded.data.deleted.contains_key("recent"));
    }

    // -- store operations --

    #[test]
    fn archive_sets_the_reason_and_clears_unread() {
        let mut store = Store::empty(PathBuf::from("/nonexistent/sessions.json"));
        store.insert(record("a", 1));
        assert!(store.set_unread("a", true, 5));

        assert!(store.archive("a", ArchiveReason::Closed, 9_000));
        let archived = store.get("a").unwrap();
        assert_eq!(archived.archived_at, Some(9));
        assert_eq!(archived.archive_reason, Some(ArchiveReason::Closed));
        assert_eq!(archived.archive_changed_at, 9_000);
        assert!(!archived.unread);
        assert_eq!(archived.unread_changed_at, 9_000);

        assert!(!store.archive("a", ArchiveReason::User, 9_500));
        assert_eq!(
            store.get("a").unwrap().archive_reason,
            Some(ArchiveReason::Closed)
        );
    }

    #[test]
    fn unarchive_clears_the_reason_and_stamps_the_change() {
        let mut store = Store::empty(PathBuf::from("/nonexistent/sessions.json"));
        store.insert(record("a", 1));
        store.archive("a", ArchiveReason::User, 5_000);

        assert!(store.unarchive("a", 8_000));
        let record = store.get("a").unwrap();
        assert_eq!(record.archived_at, None);
        assert_eq!(record.archive_reason, None);
        assert_eq!(record.archive_changed_at, 8_000);
        assert!(!store.unarchive("a", 9_000));
    }

    #[test]
    fn operations_on_unknown_ids_change_nothing() {
        let mut store = Store::empty(PathBuf::from("/nonexistent/sessions.json"));
        assert!(!store.archive("zzz", ArchiveReason::User, 1));
        assert!(!store.unarchive("zzz", 1));
        assert!(!store.set_unread("zzz", true, 1));
        assert!(!store.refresh("zzz", Some("t".to_string()), None, None));
        assert!(!store.remove("zzz", 1));
        assert!(!store.has_unsaved_intent());
    }

    #[test]
    fn intent_and_metadata_changes_set_separate_unsaved_flags() {
        let mut store = Store::empty(PathBuf::from("/nonexistent/sessions.json"));
        store.insert(record("a", 1));
        store.unsaved = Unsaved::default();

        assert!(store.refresh("a", Some("new".to_string()), None, Some(9)));
        assert!(store.has_unsaved_metadata());
        assert!(!store.has_unsaved_intent());

        assert!(store.set_unread("a", true, 5));
        assert!(store.has_unsaved_intent());
    }

    #[test]
    fn refresh_with_identical_values_is_not_a_change() {
        let mut store = Store::empty(PathBuf::from("/nonexistent/sessions.json"));
        store.insert(record("a", 1));
        store.unsaved = Unsaved::default();
        assert!(!store.refresh("a", Some("title a".to_string()), None, Some(1)));
        assert!(!store.has_unsaved_metadata());
    }

    // -- persistence --

    #[test]
    fn load_missing_file_is_none() {
        let dir = TempDir::new("missing");
        assert!(Store::load(dir.store_path()).unwrap().is_none());
    }

    #[test]
    fn load_corrupt_file_is_an_error() {
        let dir = TempDir::new("corrupt");
        std::fs::write(dir.store_path(), "{not json").unwrap();
        assert!(Store::load(dir.store_path()).is_err());
    }

    #[test]
    fn save_then_load_round_trips_and_leaves_no_temp_file() {
        let dir = TempDir::new("roundtrip");
        let mut store = Store::empty(dir.store_path());
        let mut stored = archived(record("a", 3), 2_000, ArchiveReason::User);
        stored.origin = Origin::ImportedDesktop;
        store.insert(stored.clone());
        store.save(5).unwrap();
        assert!(!store.has_unsaved_intent());

        let loaded = Store::load(dir.store_path()).unwrap().unwrap();
        assert_eq!(loaded.get("a"), Some(&stored));
        let entries: Vec<_> = std::fs::read_dir(&dir.0).unwrap().collect();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn files_written_before_per_field_timestamps_still_load() {
        let dir = TempDir::new("legacy");
        std::fs::write(
            dir.store_path(),
            r#"{"sessions":[
              {"sessionId":"live","cwd":"/w","title":"t","createdAt":100,"lastActivity":150,
               "archivedAt":null,"unread":true,"origin":"detected","updatedAt":170},
              {"sessionId":"gone","cwd":"/w","title":"","createdAt":100,"lastActivity":150,
               "archivedAt":160,"unread":false,"origin":"imported_cli","updatedAt":170}
            ]}"#,
        )
        .unwrap();

        let store = Store::load(dir.store_path()).unwrap().unwrap();
        let live = store.get("live").unwrap();
        assert_eq!(live.archived_at, None);
        assert_eq!(live.archive_reason, None);
        assert_eq!(live.archive_changed_at, 100_000);
        assert!(live.unread);
        assert_eq!(live.unread_changed_at, 100_000);
        let gone = store.get("gone").unwrap();
        assert_eq!(gone.archived_at, Some(160));
        assert_eq!(gone.archive_reason, None);
        assert_eq!(gone.archive_changed_at, 160_000);
        assert!(store.data.deleted.is_empty());
    }

    #[test]
    fn deleting_removes_the_record_from_disk_on_save() {
        let dir = TempDir::new("delete");
        let mut store = Store::empty(dir.store_path());
        store.insert(record("a", 1));
        store.insert(record("b", 1));
        store.save(2).unwrap();

        assert!(store.remove("a", 5));
        store.save(6).unwrap();

        let loaded = Store::load(dir.store_path()).unwrap().unwrap();
        assert!(loaded.get("a").is_none());
        assert!(loaded.get("b").is_some());
    }

    #[test]
    fn instances_saving_different_sessions_both_survive() {
        let dir = TempDir::new("concurrent-distinct");
        let mut first = Store::empty(dir.store_path());
        let mut second = Store::empty(dir.store_path());

        first.insert(record("a", 1));
        first.save(2).unwrap();
        second.insert(record("b", 1));
        second.save(3).unwrap();

        assert!(second.get("a").is_some(), "save adopts the merged result");
        first.sync().unwrap();
        assert!(first.get("b").is_some());
        let loaded = Store::load(dir.store_path()).unwrap().unwrap();
        assert_eq!(loaded.records().count(), 2);
    }

    #[test]
    fn instances_changing_different_fields_of_one_session_keep_both_changes() {
        let dir = TempDir::new("concurrent-same");
        let mut first = Store::empty(dir.store_path());
        first.insert(record("a", 1));
        first.save(2).unwrap();
        let mut second = Store::load(dir.store_path()).unwrap().unwrap();

        first.archive("a", ArchiveReason::User, 5_000);
        second.set_unread("a", true, 6_000);
        second.save(6_001).unwrap();
        first.save(6_002).unwrap();

        let merged = first.get("a").unwrap();
        assert!(merged.archived_at.is_some());
        assert!(
            merged.unread,
            "archive of the other instance didn't clear it"
        );
        let loaded = Store::load(dir.store_path()).unwrap().unwrap();
        assert_eq!(loaded.get("a"), Some(merged));
    }

    #[test]
    fn sync_skips_reading_while_the_file_is_unchanged() {
        let dir = TempDir::new("sync-gate");
        let mut store = Store::empty(dir.store_path());
        store.insert(record("a", 1));
        store.save(2).unwrap();

        // Garbage with the same length and mtime: only a re-read would notice.
        let original = std::fs::metadata(dir.store_path()).unwrap();
        std::fs::write(dir.store_path(), "x".repeat(original.len() as usize)).unwrap();
        std::fs::File::options()
            .write(true)
            .open(dir.store_path())
            .unwrap()
            .set_modified(original.modified().unwrap())
            .unwrap();
        assert!(store.sync().is_ok(), "unchanged stamp: not read");

        std::fs::write(dir.store_path(), "{not json").unwrap();
        assert!(store.sync().is_err(), "a changed file is read");
        assert!(
            store.sync().is_ok(),
            "and a bad one is reported once, not every tick"
        );
    }
}
