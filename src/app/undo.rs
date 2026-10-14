use std::collections::VecDeque;

use crate::sessions::record::ArchiveSnapshot;
use crate::sessions::view::ViewConfig;

const CAPACITY: usize = 50;

pub enum UndoEntry {
    /// `snapshot` is the session's state before `a` ran.
    ArchiveToggle {
        session_id: String,
        snapshot: ArchiveSnapshot,
    },
    /// The view config before one change in the `S` overlay or the grouping cycle.
    ViewChange { prior: ViewConfig },
}

impl UndoEntry {
    /// Toast shown once the entry has been undone.
    pub fn undone_message(&self) -> &'static str {
        match self {
            UndoEntry::ArchiveToggle { snapshot, .. } if snapshot.archived_at.is_none() => {
                "Undid archive"
            }
            UndoEntry::ArchiveToggle { .. } => "Undid unarchive",
            UndoEntry::ViewChange { .. } => "Undid view change",
        }
    }
}

/// In-memory only; the oldest entry is dropped once it holds `CAPACITY`.
#[derive(Default)]
pub struct UndoStack {
    entries: VecDeque<UndoEntry>,
}

impl UndoStack {
    pub fn push(&mut self, entry: UndoEntry) {
        if self.entries.len() == CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// Call after a view change was applied. A change that left the config as it was isn't
    /// undoable.
    pub fn push_view_change(&mut self, prior: ViewConfig, current: ViewConfig) {
        if prior != current {
            self.push(UndoEntry::ViewChange { prior });
        }
    }

    pub fn pop(&mut self) -> Option<UndoEntry> {
        self.entries.pop_back()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::record::ArchiveReason;
    use crate::sessions::view::{Grouping, StatusFilter};

    fn archive_entry_id(entry: Option<UndoEntry>) -> String {
        match entry.expect("stack was empty") {
            UndoEntry::ArchiveToggle { session_id, .. } => session_id,
            UndoEntry::ViewChange { .. } => panic!("expected an archive entry"),
        }
    }

    fn entry(id: &str, archived_at: Option<u64>) -> UndoEntry {
        UndoEntry::ArchiveToggle {
            session_id: id.to_string(),
            snapshot: ArchiveSnapshot {
                archived_at,
                archive_reason: archived_at.map(|_| ArchiveReason::User),
                unread: false,
            },
        }
    }

    fn grouped_by(grouping: Grouping) -> ViewConfig {
        ViewConfig {
            grouping,
            ..ViewConfig::default()
        }
    }

    #[test]
    fn pops_newest_first_then_runs_empty() {
        let mut stack = UndoStack::default();
        stack.push(entry("a", None));
        stack.push(entry("b", None));

        assert_eq!(archive_entry_id(stack.pop()), "b");
        assert_eq!(archive_entry_id(stack.pop()), "a");
        assert!(stack.pop().is_none());
    }

    #[test]
    fn drops_the_oldest_entry_past_capacity() {
        let mut stack = UndoStack::default();
        for i in 0..CAPACITY + 3 {
            stack.push(entry(&format!("s{i}"), None));
        }

        assert_eq!(stack.len(), CAPACITY);
        assert_eq!(archive_entry_id(stack.pop()), format!("s{}", CAPACITY + 2));
        let mut oldest = None;
        while let Some(entry) = stack.pop() {
            oldest = Some(entry);
        }
        assert_eq!(archive_entry_id(oldest), "s3");
    }

    #[test]
    fn message_follows_what_the_undone_action_was() {
        assert_eq!(entry("a", None).undone_message(), "Undid archive");
        assert_eq!(entry("a", Some(5)).undone_message(), "Undid unarchive");
        let view = UndoEntry::ViewChange {
            prior: ViewConfig::default(),
        };
        assert_eq!(view.undone_message(), "Undid view change");
    }

    #[test]
    fn view_changes_share_one_chronological_stack_with_archives() {
        let mut stack = UndoStack::default();
        let before = ViewConfig::default();
        let after = grouped_by(Grouping::Folder);
        stack.push(entry("a", None));
        stack.push_view_change(before, after);

        match stack.pop().expect("view entry") {
            UndoEntry::ViewChange { prior } => assert_eq!(prior, before),
            UndoEntry::ArchiveToggle { .. } => panic!("expected the view entry first"),
        }
        assert_eq!(archive_entry_id(stack.pop()), "a");
    }

    #[test]
    fn a_view_change_that_changed_nothing_is_not_recorded() {
        let mut stack = UndoStack::default();
        let view = ViewConfig {
            status: StatusFilter::Archived,
            ..ViewConfig::default()
        };
        stack.push_view_change(view, view);

        assert_eq!(stack.len(), 0);
    }
}
