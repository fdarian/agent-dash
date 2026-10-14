use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::keys::apply_text_input;
use crate::sessions::record::SessionRecord;
use crate::ui::list_nav::{nav_action, step, NavAction, NavMode};
use crate::worktree::{RepoInfo, WorktreeRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Folder,
    Worktree,
    Name,
    Base,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Submission {
    pub cwd: String,
    pub worktree: Option<WorktreeRequest>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Pending,
    Closed,
    Submit(Submission),
}

/// Reads branches and worktrees of a folder. Injected so the dialog stays free of git.
pub type RepoLoader<'a> = &'a dyn Fn(&str) -> Result<RepoInfo>;

/// What the typed worktree name means for the repository at the resolved folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameStatus {
    /// Blank: a name is generated.
    Random,
    /// Not a local branch.
    New,
    /// A local branch checked out in the worktree at this path.
    ExistingWithWorktree(String),
    ExistingWithoutWorktree,
}

/// Whether the dialog offers the Worktree checkbox, and its starting state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeOption {
    Unavailable,
    Available {
        checked: bool,
        /// Whether `worktree.checkout` is configured, which existing branches without a
        /// worktree need.
        can_checkout: bool,
    },
}

/// `c` dialog: pick a folder, optionally get a git worktree there first.
///
/// Name, Base and Folder work like zsh menu-complete: `text` is what is shown and submitted,
/// `query` is what was last typed, and the shared suggestion list is filtered by `query` so
/// cycling through it doesn't change the list. Moving the highlight writes the suggestion
/// into `text`; leaving the first item restores `query`.
pub struct NewSessionDialog {
    /// Every folder (as the Folder grouping defines it) in the store, most recently active
    /// first.
    folders: Vec<String>,
    /// The default folder: shown as a placeholder and used while Folder is empty.
    preselected: Option<String>,
    pub folder: String,
    pub folder_cursor: usize,
    folder_query: String,
    folder_highlight: Option<usize>,
    folder_restore: String,
    pub focus: Field,
    worktree_available: bool,
    can_checkout: bool,
    pub worktree: bool,
    /// May stay empty; the caller then generates one.
    pub name: String,
    pub name_cursor: usize,
    name_query: String,
    name_highlight: Option<usize>,
    name_restore: String,
    pub base: String,
    pub base_cursor: usize,
    base_query: String,
    base_highlight: Option<usize>,
    base_restore: String,
    base_edited: bool,
    /// The resolved folder `repo` was last loaded for.
    repo_folder: Option<String>,
    repo: RepoInfo,
    /// Why the repository couldn't be read.
    pub branch_error: Option<String>,
    /// Submit validation, cleared by the next key.
    pub error: Option<String>,
}

impl NewSessionDialog {
    pub fn open<'a>(
        records: impl Iterator<Item = &'a SessionRecord>,
        selected_folder: Option<&str>,
        worktree: WorktreeOption,
        load: RepoLoader,
    ) -> Self {
        let (checked, can_checkout) = match worktree {
            WorktreeOption::Unavailable => (false, false),
            WorktreeOption::Available {
                checked,
                can_checkout,
            } => (checked, can_checkout),
        };
        let folders = known_folders(records);
        // Without a selected session the most recent folder is the default.
        let preselected = match selected_folder {
            Some(folder) => Some(folder.to_string()),
            None => folders.first().cloned(),
        };
        let mut dialog = NewSessionDialog {
            folders,
            preselected,
            folder: String::new(),
            folder_cursor: 0,
            folder_query: String::new(),
            folder_highlight: None,
            folder_restore: String::new(),
            focus: Field::Folder,
            worktree_available: worktree != WorktreeOption::Unavailable,
            can_checkout,
            worktree: checked,
            name: String::new(),
            name_cursor: 0,
            name_query: String::new(),
            name_highlight: None,
            name_restore: String::new(),
            base: String::new(),
            base_cursor: 0,
            base_query: String::new(),
            base_highlight: None,
            base_restore: String::new(),
            base_edited: false,
            repo_folder: None,
            repo: RepoInfo::default(),
            branch_error: None,
            error: None,
        };
        dialog.sync_repo(load);
        dialog
    }

    pub fn worktree_available(&self) -> bool {
        self.worktree_available
    }

    /// The default folder, shown dim in the empty Folder input: the selected session's, else the
    /// most recent one.
    pub fn folder_placeholder(&self) -> Option<&str> {
        self.preselected.as_deref()
    }

    /// The focusable fields in Tab order.
    pub fn fields(&self) -> Vec<Field> {
        let mut fields = vec![Field::Folder];
        if self.worktree_available {
            fields.push(Field::Worktree);
            if self.worktree {
                fields.extend([Field::Name, Field::Base]);
            }
        }
        fields
    }

    /// Known folders containing the typed query, case-insensitively.
    fn filtered_folders(&self) -> Vec<&str> {
        let needle = expand_tilde(&self.folder_query).to_lowercase();
        self.folders
            .iter()
            .map(String::as_str)
            .filter(|folder| folder.to_lowercase().contains(&needle))
            .collect()
    }

    /// The shared list's contents, which follow the focused field.
    pub fn suggestions(&self) -> Vec<&str> {
        match self.focus {
            Field::Folder => self.filtered_folders(),
            Field::Worktree => Vec::new(),
            Field::Name => matching(&self.repo.local, &self.name_query),
            Field::Base => {
                let needle = self.base_query.trim().to_lowercase();
                self.repo
                    .local
                    .iter()
                    .chain(&self.repo.remote)
                    .map(String::as_str)
                    .filter(|branch| branch.to_lowercase().contains(&needle))
                    .collect()
            }
        }
    }

    /// The highlighted suggestion of the focused field, if the user has cycled to one.
    pub fn highlight_index(&self) -> Option<usize> {
        match self.focus {
            Field::Folder => self.folder_highlight,
            Field::Name => self.name_highlight,
            Field::Base => self.base_highlight,
            Field::Worktree => None,
        }
    }

    /// The cycled folder, else the typed directory, else the preselected folder.
    pub fn resolved_folder(&self) -> Option<String> {
        if let Some(index) = self.folder_highlight {
            return self
                .filtered_folders()
                .get(index)
                .map(|folder| folder.to_string());
        }
        let typed = self.folder.trim();
        if typed.is_empty() {
            return self.preselected.clone();
        }
        let expanded = expand_tilde(typed);
        Path::new(&expanded).is_dir().then_some(expanded)
    }

    pub fn name_status(&self) -> NameStatus {
        let name = self.name.trim();
        if name.is_empty() {
            NameStatus::Random
        } else if !self.repo.local.iter().any(|branch| branch == name) {
            NameStatus::New
        } else {
            match self.repo.worktrees.get(name) {
                Some(path) => NameStatus::ExistingWithWorktree(path.clone()),
                None => NameStatus::ExistingWithoutWorktree,
            }
        }
    }

    /// What the typed name will do, shown under the fields while the worktree is on.
    pub fn status_line(&self) -> Option<String> {
        if !self.worktree {
            return None;
        }
        let base = self.base.trim();
        let from = if base.is_empty() { "(no base)" } else { base };
        Some(match self.name_status() {
            NameStatus::Random => format!("random new branch from {from}"),
            NameStatus::New => format!("new branch from {from}"),
            NameStatus::ExistingWithWorktree(_) => {
                "already has a worktree \u{2192} open it".to_string()
            }
            NameStatus::ExistingWithoutWorktree => "existing branch".to_string(),
        })
    }

    pub fn name_dimmed(&self) -> bool {
        !self.worktree
    }

    /// Base only matters for a new branch.
    pub fn base_dimmed(&self) -> bool {
        !self.worktree
            || matches!(
                self.name_status(),
                NameStatus::ExistingWithWorktree(_) | NameStatus::ExistingWithoutWorktree
            )
    }

    pub fn handle_key(&mut self, key: KeyEvent, load: RepoLoader) -> Outcome {
        self.error = None;
        let outcome = self.dispatch(key);
        if outcome == Outcome::Pending {
            self.sync_repo(load);
        }
        outcome
    }

    fn dispatch(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Tab => {
                self.move_focus(true);
                return Outcome::Pending;
            }
            KeyCode::BackTab => {
                self.move_focus(false);
                return Outcome::Pending;
            }
            _ => {}
        }
        if let Some(nav) = nav_action(key, NavMode::TextInput) {
            match nav {
                NavAction::Select => return self.submit(),
                NavAction::Close => return Outcome::Closed,
                NavAction::Next | NavAction::Prev => self.move_highlight(nav),
            }
            return Outcome::Pending;
        }
        if self.focus == Field::Worktree {
            if key.code == KeyCode::Char(' ') && key.modifiers == KeyModifiers::NONE {
                self.toggle_worktree();
            }
            return Outcome::Pending;
        }
        let edited = {
            let (text, cursor) = self.text_mut();
            apply_text_input(text, cursor, key)
        };
        if edited {
            self.set_query_from_text();
        }
        Outcome::Pending
    }

    fn text_mut(&mut self) -> (&mut String, &mut usize) {
        match self.focus {
            Field::Folder | Field::Worktree => (&mut self.folder, &mut self.folder_cursor),
            Field::Name => (&mut self.name, &mut self.name_cursor),
            Field::Base => (&mut self.base, &mut self.base_cursor),
        }
    }

    fn restore_mut(&mut self) -> &mut String {
        match self.focus {
            Field::Folder | Field::Worktree => &mut self.folder_restore,
            Field::Name => &mut self.name_restore,
            Field::Base => &mut self.base_restore,
        }
    }

    /// An edit makes the text the new query and drops the highlight; editing after cycling
    /// continues from the cycled text.
    fn set_query_from_text(&mut self) {
        match self.focus {
            Field::Folder | Field::Worktree => {
                self.folder_query = self.folder.clone();
                self.folder_highlight = None;
            }
            Field::Name => {
                self.name_query = self.name.clone();
                self.name_highlight = None;
            }
            Field::Base => {
                self.base_query = self.base.clone();
                self.base_highlight = None;
                self.base_edited = true;
            }
        }
    }

    /// Down starts at the first match (Folder: the preselected folder when listed) and then
    /// moves on, clamping at the end; Up from the first match restores the text the field had
    /// before cycling started.
    fn move_highlight(&mut self, nav: NavAction) {
        if self.focus == Field::Worktree {
            return;
        }
        let list: Vec<String> = self
            .suggestions()
            .iter()
            .map(|suggestion| suggestion.to_string())
            .collect();
        let next = match (self.highlight_index(), nav) {
            (None, NavAction::Next) if !list.is_empty() => Some(self.first_index(&list)),
            (Some(index), NavAction::Next) => Some(step(index, list.len(), nav)),
            (Some(index), NavAction::Prev) if index > 0 => Some(index - 1),
            (Some(_), NavAction::Prev) => None,
            _ => return,
        };
        let focus = self.focus;
        if self.highlight_index().is_none() {
            let before_cycling = self.text_mut().0.clone();
            *self.restore_mut() = before_cycling;
        }
        let shown = match next {
            Some(index) => list[index].clone(),
            None => self.restore_mut().clone(),
        };
        let (text, cursor) = self.text_mut();
        *cursor = shown.chars().count();
        *text = shown;
        match focus {
            Field::Folder | Field::Worktree => self.folder_highlight = next,
            Field::Name => self.name_highlight = next,
            Field::Base => {
                self.base_highlight = next;
                self.base_edited = true;
            }
        }
    }

    fn first_index(&self, list: &[String]) -> usize {
        match (self.focus, &self.preselected) {
            (Field::Folder, Some(preselected)) => list
                .iter()
                .position(|folder| folder == preselected)
                .unwrap_or(0),
            _ => 0,
        }
    }

    fn move_focus(&mut self, forward: bool) {
        let fields = self.fields();
        let current = fields
            .iter()
            .position(|field| *field == self.focus)
            .expect("the focused field is always one of the shown fields");
        let next = if forward {
            (current + 1) % fields.len()
        } else {
            (current + fields.len() - 1) % fields.len()
        };
        self.focus = fields[next];
    }

    fn toggle_worktree(&mut self) {
        self.worktree = !self.worktree;
        if self.worktree {
            self.repo_folder = None;
        }
    }

    /// Reloads the repository data when the resolved folder changed, and prefills Base with its
    /// current branch until the user edits or cycles Base.
    fn sync_repo(&mut self, load: RepoLoader) {
        if !self.worktree {
            return;
        }
        let resolved = self.resolved_folder();
        if resolved == self.repo_folder {
            return;
        }
        self.repo_folder = resolved.clone();
        self.branch_error = None;
        self.repo = RepoInfo::default();
        if let Some(folder) = resolved {
            match load(&folder) {
                Ok(info) => self.repo = info,
                Err(err) => {
                    self.branch_error = Some(format!("Can't read branches of {folder}: {err:#}"));
                }
            }
        }
        if !self.base_edited {
            self.base = self.repo.current.clone();
            // Not typed, so the list stays unfiltered and Down offers every branch.
            self.base_query.clear();
            self.base_highlight = None;
            self.base_cursor = self.base.chars().count();
        }
    }

    fn submit(&mut self) -> Outcome {
        let Some(cwd) = self.resolved_folder() else {
            self.error = Some("No folder selected".to_string());
            return Outcome::Pending;
        };
        if !self.worktree {
            return Outcome::Submit(Submission {
                cwd,
                worktree: None,
            });
        }
        let name = self.name.trim().to_string();
        let worktree = match self.name_status() {
            NameStatus::Random | NameStatus::New => {
                let base = self.base.trim();
                if base.is_empty() {
                    self.error = Some("Base branch is required".to_string());
                    self.focus = Field::Base;
                    return Outcome::Pending;
                }
                WorktreeRequest::New {
                    name,
                    base: base.to_string(),
                }
            }
            NameStatus::ExistingWithWorktree(path) => {
                WorktreeRequest::Existing { branch: name, path }
            }
            NameStatus::ExistingWithoutWorktree => {
                if !self.can_checkout {
                    self.error = Some("no worktree.checkout configured".to_string());
                    self.focus = Field::Name;
                    return Outcome::Pending;
                }
                WorktreeRequest::Checkout { branch: name }
            }
        };
        Outcome::Submit(Submission {
            cwd,
            worktree: Some(worktree),
        })
    }
}

fn matching<'a>(branches: &'a [String], typed: &str) -> Vec<&'a str> {
    let needle = typed.trim().to_lowercase();
    branches
        .iter()
        .map(String::as_str)
        .filter(|branch| branch.to_lowercase().contains(&needle))
        .collect()
}

fn known_folders<'a>(records: impl Iterator<Item = &'a SessionRecord>) -> Vec<String> {
    let mut latest: HashMap<&str, u64> = HashMap::new();
    for record in records {
        let activity = latest.entry(record.folder_key()).or_insert(0);
        *activity = (*activity).max(record.last_activity);
    }
    let mut folders: Vec<(&str, u64)> = latest.into_iter().collect();
    folders.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    folders
        .into_iter()
        .map(|(folder, _)| folder.to_string())
        .collect()
}

fn expand_tilde(text: &str) -> String {
    let home = || {
        dirs::home_dir()
            .expect("home directory not found")
            .to_string_lossy()
            .into_owned()
    };
    if text == "~" {
        home()
    } else if let Some(rest) = text.strip_prefix("~/") {
        format!("{}/{rest}", home())
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::sessions::record::Origin;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn record(cwd: &str, last_activity: u64) -> SessionRecord {
        SessionRecord::new(
            format!("id-{cwd}-{last_activity}"),
            cwd.to_string(),
            String::new(),
            0,
            last_activity,
            Origin::Detected,
            0,
        )
    }

    fn dialog(cwds: &[(&str, u64)], selected: Option<&str>, worktree: bool) -> NewSessionDialog {
        let records: Vec<SessionRecord> = cwds.iter().map(|(c, a)| record(c, *a)).collect();
        NewSessionDialog::open(
            records.iter(),
            selected,
            if worktree {
                WorktreeOption::Available {
                    checked: false,
                    can_checkout: true,
                }
            } else {
                WorktreeOption::Unavailable
            },
            &repo_of,
        )
    }

    fn repo_of(folder: &str) -> Result<RepoInfo> {
        Ok(RepoInfo {
            current: format!("branch-of:{folder}"),
            local: vec![
                "main".into(),
                "feat-a".into(),
                "Feat-B".into(),
                "docs".into(),
            ],
            remote: vec!["origin/main".into(), "origin/feat-c".into()],
            worktrees: HashMap::from([
                ("main".to_string(), folder.to_string()),
                ("Feat-B".to_string(), "/wt/feat-b".to_string()),
            ]),
        })
    }

    fn send(dialog: &mut NewSessionDialog, key: KeyEvent) -> Outcome {
        dialog.handle_key(key, &repo_of)
    }

    fn type_text(dialog: &mut NewSessionDialog, text: &str) {
        for c in text.chars() {
            send(dialog, press(KeyCode::Char(c)));
        }
    }

    /// Three folders, with `/work/web` (the most recent) preselected.
    fn projects() -> NewSessionDialog {
        dialog(
            &[
                ("/work/api", 10),
                ("/work/web", 30),
                ("/Work/Docs", 20),
                ("/work/api", 5),
            ],
            Some("/work/web"),
            true,
        )
    }

    fn down(dialog: &mut NewSessionDialog) {
        send(dialog, press(KeyCode::Down));
    }

    fn up(dialog: &mut NewSessionDialog) {
        send(dialog, press(KeyCode::Up));
    }

    fn tab(dialog: &mut NewSessionDialog) {
        send(dialog, press(KeyCode::Tab));
    }

    fn enter(dialog: &mut NewSessionDialog) -> Outcome {
        send(dialog, press(KeyCode::Enter))
    }

    fn open_with(records: &[SessionRecord], selected: Option<&str>) -> NewSessionDialog {
        NewSessionDialog::open(
            records.iter(),
            selected,
            WorktreeOption::Unavailable,
            &repo_of,
        )
    }

    fn submit_cwd(cwd: &str) -> Outcome {
        Outcome::Submit(Submission {
            cwd: cwd.to_string(),
            worktree: None,
        })
    }

    fn submit_new(cwd: &str, name: &str, base: &str) -> Outcome {
        Outcome::Submit(Submission {
            cwd: cwd.to_string(),
            worktree: Some(WorktreeRequest::New {
                name: name.to_string(),
                base: base.to_string(),
            }),
        })
    }

    #[test]
    fn folders_are_distinct_and_ordered_by_latest_activity() {
        assert_eq!(
            projects().suggestions(),
            vec!["/work/web", "/Work/Docs", "/work/api"]
        );
    }

    #[test]
    fn archived_and_unarchived_records_both_contribute_folders() {
        let mut archived = record("/old", 1);
        archived.archived_at = Some(2);
        let records = [archived, record("/new", 2)];
        let dialog = open_with(&records, None);
        assert_eq!(dialog.suggestions(), vec!["/new", "/old"]);
    }

    /// A record of the repo at `/code/app`, whichever worktree or subdirectory it ran in.
    fn in_app_repo(cwd: &str, last_activity: u64) -> SessionRecord {
        let mut record = record(cwd, last_activity);
        record.repo_root = Some("/code/app".to_string());
        record
    }

    #[test]
    fn worktrees_and_subdirectories_collapse_into_their_repo_root() {
        let records = [
            in_app_repo("/code/app", 10),
            in_app_repo("/wt/app-feature", 40),
            in_app_repo("/code/app/src", 20),
            record("/notes", 30),
        ];
        let dialog = open_with(&records, None);
        assert_eq!(dialog.suggestions(), vec!["/code/app", "/notes"]);
    }

    #[test]
    fn a_repo_folder_is_as_recent_as_its_newest_worktree() {
        let records = [
            in_app_repo("/code/app", 10),
            record("/notes", 30),
            in_app_repo("/wt/app-feature", 40),
        ];
        let dialog = open_with(&records, None);
        assert_eq!(dialog.suggestions(), vec!["/code/app", "/notes"]);

        let records = [in_app_repo("/code/app", 10), record("/notes", 30)];
        let dialog = open_with(&records, None);
        assert_eq!(dialog.suggestions(), vec!["/notes", "/code/app"]);
    }

    #[test]
    fn a_deleted_worktree_still_belongs_to_its_repos_folder() {
        let records = [
            in_app_repo("/wt/deleted-branch", 50),
            in_app_repo("/code/app", 10),
        ];
        let dialog = open_with(&records, None);
        assert_eq!(dialog.suggestions(), vec!["/code/app"]);
    }

    #[test]
    fn opens_empty_with_the_selected_sessions_folder_as_placeholder_and_default() {
        let records = [
            record("/notes", 30),
            in_app_repo("/wt/app-feature", 20),
            record("/other", 10),
        ];
        let dialog = open_with(&records, Some(records[1].folder_key()));
        assert_eq!(dialog.folder, "");
        assert_eq!(dialog.folder_placeholder(), Some("/code/app"));
        assert_eq!(dialog.highlight_index(), None);
        assert_eq!(dialog.resolved_folder().as_deref(), Some("/code/app"));
    }

    #[test]
    fn enter_on_open_submits_the_preselected_folder() {
        let mut dialog = projects();
        dialog.worktree = false;
        assert_eq!(enter(&mut dialog), submit_cwd("/work/web"));
    }

    #[test]
    fn without_a_selection_the_most_recent_folder_is_the_default() {
        let records = [record("/a", 3), record("/b", 2)];
        let mut dialog = open_with(&records, None);
        assert_eq!(dialog.folder_placeholder(), Some("/a"));
        assert_eq!(dialog.resolved_folder().as_deref(), Some("/a"));
        assert_eq!(enter(&mut dialog), submit_cwd("/a"));
    }

    #[test]
    fn without_a_selection_down_starts_at_the_most_recent_folder() {
        let records = [record("/a", 3), record("/b", 2)];
        let mut dialog = open_with(&records, None);
        down(&mut dialog);
        assert_eq!(
            (dialog.folder.as_str(), dialog.highlight_index()),
            ("/a", Some(0))
        );
        down(&mut dialog);
        assert_eq!(dialog.folder, "/b");
    }

    #[test]
    fn with_no_known_folder_at_all_there_is_nothing_to_submit() {
        let mut dialog = open_with(&[], None);
        assert_eq!(dialog.folder_placeholder(), None);
        assert_eq!(enter(&mut dialog), Outcome::Pending);
        assert_eq!(dialog.error.as_deref(), Some("No folder selected"));
        down(&mut dialog);
        assert_eq!(dialog.folder, "");
    }

    #[test]
    fn down_starts_at_the_preselected_folder_then_moves_on_and_clamps() {
        let records = [record("/a", 3), record("/b", 2), record("/c", 1)];
        let mut dialog = open_with(&records, Some("/b"));
        down(&mut dialog);
        assert_eq!(
            (dialog.folder.as_str(), dialog.highlight_index()),
            ("/b", Some(1))
        );
        down(&mut dialog);
        down(&mut dialog);
        assert_eq!(
            (dialog.folder.as_str(), dialog.highlight_index()),
            ("/c", Some(2))
        );
        up(&mut dialog);
        assert_eq!(dialog.folder, "/b");
        up(&mut dialog);
        assert_eq!(dialog.folder, "/a");
        assert_eq!(dialog.folder_cursor, 2);
    }

    #[test]
    fn up_from_the_first_item_restores_the_typed_query() {
        let mut dialog = projects();
        type_text(&mut dialog, "/work/");
        down(&mut dialog);
        assert_eq!(dialog.folder, "/work/web");
        up(&mut dialog);
        assert_eq!(dialog.folder, "/work/");
        assert_eq!(dialog.highlight_index(), None);
        up(&mut dialog);
        assert_eq!(dialog.folder, "/work/");
    }

    #[test]
    fn up_without_a_highlight_does_nothing() {
        let mut dialog = projects();
        up(&mut dialog);
        assert_eq!(
            (dialog.folder.as_str(), dialog.highlight_index()),
            ("", None)
        );
    }

    #[test]
    fn the_list_is_filtered_by_the_query_so_cycling_does_not_collapse_it() {
        let mut dialog = projects();
        type_text(&mut dialog, "work/");
        let before = dialog.suggestions().len();
        assert_eq!(before, 3);
        down(&mut dialog);
        down(&mut dialog);
        assert_eq!(dialog.folder, "/Work/Docs");
        assert_eq!(dialog.suggestions().len(), before);
        type_text(&mut dialog, "x");
        assert!(dialog.suggestions().is_empty());
    }

    #[test]
    fn filtering_is_a_case_insensitive_substring() {
        let mut dialog = projects();
        type_text(&mut dialog, "WORK/a");
        assert_eq!(dialog.suggestions(), vec!["/work/api"]);
    }

    #[test]
    fn editing_after_cycling_continues_from_the_cycled_text() {
        let mut dialog = projects();
        type_text(&mut dialog, "/wo");
        down(&mut dialog);
        down(&mut dialog);
        assert_eq!(dialog.folder, "/Work/Docs");
        type_text(&mut dialog, "/x");
        assert_eq!(dialog.folder, "/Work/Docs/x");
        assert_eq!(dialog.highlight_index(), None);
        assert!(dialog.suggestions().is_empty());
        send(&mut dialog, press(KeyCode::Backspace));
        send(&mut dialog, press(KeyCode::Backspace));
        assert_eq!(dialog.suggestions(), vec!["/Work/Docs"]);
    }

    #[test]
    fn j_and_k_are_typed_into_the_folder() {
        let mut dialog = projects();
        type_text(&mut dialog, "jk");
        assert_eq!(dialog.folder, "jk");
    }

    #[test]
    fn arrows_and_ctrl_keys_both_cycle() {
        let mut dialog = projects();
        send(&mut dialog, ctrl('n'));
        send(&mut dialog, ctrl('n'));
        assert_eq!(dialog.folder, "/Work/Docs");
        send(&mut dialog, ctrl('p'));
        assert_eq!(dialog.folder, "/work/web");
    }

    #[test]
    fn a_cycled_folder_is_submitted_even_if_it_is_not_a_directory() {
        let mut dialog = projects();
        dialog.worktree = false;
        down(&mut dialog);
        down(&mut dialog);
        assert_eq!(enter(&mut dialog), submit_cwd("/Work/Docs"));
    }

    #[test]
    fn a_typed_existing_directory_is_the_folder() {
        let tmp = std::env::temp_dir();
        let tmp = tmp.to_str().unwrap();
        let mut dialog = projects();
        type_text(&mut dialog, tmp);
        assert_eq!(dialog.resolved_folder().as_deref(), Some(tmp));
    }

    #[test]
    fn typed_text_that_is_not_a_directory_is_no_folder_even_with_a_preselection() {
        let mut dialog = projects();
        type_text(&mut dialog, "/definitely/not/here");
        assert_eq!(dialog.resolved_folder(), None);
        assert_eq!(enter(&mut dialog), Outcome::Pending);
        assert_eq!(dialog.error.as_deref(), Some("No folder selected"));
        send(&mut dialog, press(KeyCode::Left));
        assert_eq!(dialog.error, None);
    }

    #[test]
    fn tilde_expands_to_the_home_directory() {
        let home = dirs::home_dir().unwrap();
        let home = home.to_str().unwrap();
        let mut dialog = projects();
        type_text(&mut dialog, "~");
        assert_eq!(dialog.resolved_folder().as_deref(), Some(home));
        assert_eq!(expand_tilde("~/code"), format!("{home}/code"));
        assert_eq!(expand_tilde("~other"), "~other");
    }

    #[test]
    fn esc_closes_from_any_field() {
        let mut dialog = projects();
        tab(&mut dialog);
        assert_eq!(send(&mut dialog, press(KeyCode::Esc)), Outcome::Closed);
    }

    #[test]
    fn tab_only_moves_focus_and_never_completes() {
        let mut dialog = projects();
        type_text(&mut dialog, "/work/");
        down(&mut dialog);
        tab(&mut dialog);
        assert_eq!(dialog.focus, Field::Worktree);
        assert_eq!(dialog.folder, "/work/web");
        send(&mut dialog, press(KeyCode::BackTab));
        assert_eq!(dialog.focus, Field::Folder);
    }

    #[test]
    fn tab_cycles_over_the_shown_fields_only() {
        let mut dialog = projects();
        tab(&mut dialog);
        assert_eq!(dialog.focus, Field::Worktree);
        tab(&mut dialog);
        assert_eq!(dialog.focus, Field::Folder);
        tab(&mut dialog);
        send(&mut dialog, press(KeyCode::Char(' ')));
        tab(&mut dialog);
        assert_eq!(dialog.focus, Field::Name);
        tab(&mut dialog);
        assert_eq!(dialog.focus, Field::Base);
        tab(&mut dialog);
        assert_eq!(dialog.focus, Field::Folder);
    }

    #[test]
    fn shift_tab_cycles_backwards() {
        let mut dialog = projects();
        send(&mut dialog, press(KeyCode::BackTab));
        assert_eq!(dialog.focus, Field::Worktree);
        send(&mut dialog, press(KeyCode::Char(' ')));
        send(&mut dialog, press(KeyCode::BackTab));
        assert_eq!(dialog.focus, Field::Folder);
        send(&mut dialog, press(KeyCode::BackTab));
        assert_eq!(dialog.focus, Field::Base);
    }

    #[test]
    fn without_a_worktree_command_the_checkbox_is_not_a_field() {
        let mut dialog = dialog(&[("/a", 1)], Some("/a"), false);
        assert_eq!(dialog.fields(), vec![Field::Folder]);
        tab(&mut dialog);
        assert_eq!(dialog.focus, Field::Folder);
        assert!(!dialog.worktree);
    }

    #[test]
    fn space_toggles_only_on_the_checkbox() {
        let mut dialog = projects();
        send(&mut dialog, press(KeyCode::Char(' ')));
        assert!(!dialog.worktree);
        assert_eq!(dialog.folder, " ");
        tab(&mut dialog);
        send(&mut dialog, press(KeyCode::Char(' ')));
        assert!(dialog.worktree);
        send(&mut dialog, press(KeyCode::Char(' ')));
        assert!(!dialog.worktree);
    }

    fn with_worktree_on() -> NewSessionDialog {
        let mut dialog = projects();
        tab(&mut dialog);
        send(&mut dialog, press(KeyCode::Char(' ')));
        dialog
    }

    fn name_focused() -> NewSessionDialog {
        let mut dialog = with_worktree_on();
        tab(&mut dialog);
        assert_eq!(dialog.focus, Field::Name);
        dialog
    }

    fn base_focused_and_cleared() -> NewSessionDialog {
        let mut dialog = name_focused();
        tab(&mut dialog);
        send(&mut dialog, ctrl('u'));
        assert_eq!(dialog.base, "");
        dialog
    }

    #[test]
    fn typing_goes_to_the_focused_input() {
        let mut dialog = name_focused();
        type_text(&mut dialog, "zzz");
        tab(&mut dialog);
        type_text(&mut dialog, "x");
        assert_eq!(dialog.name, "zzz");
        assert_eq!(dialog.base, "branch-of:/work/webx");
        assert_eq!(dialog.base_cursor, dialog.base.chars().count());
        assert_eq!(dialog.folder, "");
    }

    #[test]
    fn opening_with_the_checkbox_on_prefills_base_from_the_preselected_folder() {
        let records = [record("/a", 2), record("/b", 1)];
        let dialog = NewSessionDialog::open(
            records.iter(),
            Some("/b"),
            WorktreeOption::Available {
                checked: true,
                can_checkout: true,
            },
            &repo_of,
        );
        assert!(dialog.worktree);
        assert_eq!(dialog.base, "branch-of:/b");
        assert_eq!(dialog.base_cursor, dialog.base.chars().count());
        assert_eq!(dialog.name, "");
        assert_eq!(dialog.fields().len(), 4);
    }

    #[test]
    fn turning_the_checkbox_on_prefills_base_from_the_resolved_folder() {
        assert_eq!(with_worktree_on().base, "branch-of:/work/web");
    }

    #[test]
    fn base_follows_the_resolved_folder_until_edited() {
        let mut dialog = with_worktree_on();
        send(&mut dialog, press(KeyCode::BackTab));
        down(&mut dialog);
        down(&mut dialog);
        assert_eq!(dialog.folder, "/Work/Docs");
        assert_eq!(dialog.base, "branch-of:/Work/Docs");

        tab(&mut dialog);
        tab(&mut dialog);
        tab(&mut dialog);
        send(&mut dialog, press(KeyCode::Char('!')));
        tab(&mut dialog);
        down(&mut dialog);
        assert_eq!(dialog.base, "branch-of:/Work/Docs!");
    }

    #[test]
    fn an_edited_base_is_kept_when_the_folder_changes_or_the_checkbox_is_retoggled() {
        let mut dialog = name_focused();
        tab(&mut dialog);
        send(&mut dialog, press(KeyCode::Char('!')));
        let edited = dialog.base.clone();
        for _ in 0..3 {
            send(&mut dialog, press(KeyCode::BackTab));
        }
        down(&mut dialog);
        down(&mut dialog);
        assert_eq!(dialog.base, edited);
        tab(&mut dialog);
        send(&mut dialog, press(KeyCode::Char(' ')));
        send(&mut dialog, press(KeyCode::Char(' ')));
        assert_eq!(dialog.base, edited);
    }

    #[test]
    fn the_repository_is_only_loaded_when_the_resolved_folder_changes() {
        let calls = RefCell::new(Vec::<String>::new());
        let load = |folder: &str| {
            calls.borrow_mut().push(folder.to_string());
            Ok(RepoInfo::default())
        };
        let mut dialog = projects();
        dialog.handle_key(press(KeyCode::Tab), &load);
        assert!(calls.borrow().is_empty());
        dialog.handle_key(press(KeyCode::Char(' ')), &load);
        dialog.handle_key(press(KeyCode::Tab), &load);
        dialog.handle_key(press(KeyCode::Char('a')), &load);
        assert_eq!(*calls.borrow(), vec!["/work/web"]);
        dialog.handle_key(press(KeyCode::BackTab), &load);
        dialog.handle_key(press(KeyCode::BackTab), &load);
        dialog.handle_key(press(KeyCode::Down), &load);
        assert_eq!(*calls.borrow(), vec!["/work/web"]);
        dialog.handle_key(press(KeyCode::Down), &load);
        assert_eq!(*calls.borrow(), vec!["/work/web", "/Work/Docs"]);
    }

    #[test]
    fn a_failed_load_leaves_base_empty_and_says_why() {
        let fail = |_: &str| -> Result<RepoInfo> { Err(anyhow::anyhow!("not a git repository")) };
        let mut dialog = projects();
        dialog.handle_key(press(KeyCode::Tab), &fail);
        dialog.handle_key(press(KeyCode::Char(' ')), &fail);
        assert_eq!(dialog.base, "");
        let reason = dialog.branch_error.as_deref().unwrap();
        assert!(reason.contains("/work/web") && reason.contains("not a git repository"));
        dialog.handle_key(press(KeyCode::BackTab), &repo_of);
        dialog.handle_key(press(KeyCode::Down), &repo_of);
        dialog.handle_key(press(KeyCode::Down), &repo_of);
        assert_eq!(dialog.base, "branch-of:/Work/Docs");
        assert_eq!(dialog.branch_error, None);
    }

    #[test]
    fn worktree_submit_needs_a_base() {
        let mut dialog = name_focused();
        type_text(&mut dialog, "zzz");
        tab(&mut dialog);
        send(&mut dialog, ctrl('u'));
        assert_eq!(enter(&mut dialog), Outcome::Pending);
        assert_eq!(dialog.error.as_deref(), Some("Base branch is required"));
        assert_eq!(dialog.focus, Field::Base);
    }

    #[test]
    fn a_blank_name_is_submitted_empty_for_the_caller_to_fill_in() {
        for typed in ["", "   "] {
            let mut dialog = name_focused();
            type_text(&mut dialog, typed);
            assert_eq!(
                enter(&mut dialog),
                submit_new("/work/web", "", "branch-of:/work/web")
            );
        }
    }

    #[test]
    fn worktree_submit_carries_the_trimmed_name_and_base() {
        let mut dialog = name_focused();
        type_text(&mut dialog, " zzz ");
        assert_eq!(
            enter(&mut dialog),
            submit_new("/work/web", "zzz", "branch-of:/work/web")
        );
    }

    #[test]
    fn a_hidden_worktree_form_is_not_validated() {
        let mut dialog = with_worktree_on();
        send(&mut dialog, press(KeyCode::Char(' ')));
        assert_eq!(enter(&mut dialog), submit_cwd("/work/web"));
    }

    #[test]
    fn status_line_follows_the_typed_name() {
        let mut dialog = name_focused();
        let cases = [
            ("", "random new branch from branch-of:/work/web"),
            ("brand-new", "new branch from branch-of:/work/web"),
            ("origin/main", "new branch from branch-of:/work/web"),
            ("Feat-B", "already has a worktree \u{2192} open it"),
            ("feat-a", "existing branch"),
            ("  docs  ", "existing branch"),
        ];
        for (typed, expected) in cases {
            send(&mut dialog, ctrl('u'));
            type_text(&mut dialog, typed);
            assert_eq!(dialog.status_line().as_deref(), Some(expected), "{typed:?}");
        }
    }

    #[test]
    fn status_line_is_hidden_while_the_checkbox_is_off() {
        assert_eq!(projects().status_line(), None);
    }

    #[test]
    fn name_status_distinguishes_the_four_cases() {
        let mut dialog = name_focused();
        assert_eq!(dialog.name_status(), NameStatus::Random);
        type_text(&mut dialog, "zzz");
        assert_eq!(dialog.name_status(), NameStatus::New);
        send(&mut dialog, ctrl('u'));
        type_text(&mut dialog, "Feat-B");
        assert_eq!(
            dialog.name_status(),
            NameStatus::ExistingWithWorktree("/wt/feat-b".to_string())
        );
        send(&mut dialog, ctrl('u'));
        type_text(&mut dialog, "docs");
        assert_eq!(dialog.name_status(), NameStatus::ExistingWithoutWorktree);
    }

    #[test]
    fn rows_are_dimmed_when_they_do_not_apply() {
        let dialog = projects();
        assert!(dialog.name_dimmed() && dialog.base_dimmed());

        let mut dialog = name_focused();
        assert!(!dialog.name_dimmed() && !dialog.base_dimmed());
        type_text(&mut dialog, "brand-new");
        assert!(!dialog.base_dimmed());
        send(&mut dialog, ctrl('u'));
        type_text(&mut dialog, "docs");
        assert!(dialog.base_dimmed() && !dialog.name_dimmed());
        send(&mut dialog, ctrl('u'));
        type_text(&mut dialog, "Feat-B");
        assert!(dialog.base_dimmed());
    }

    #[test]
    fn name_and_base_cannot_be_focused_while_the_checkbox_is_off() {
        let mut dialog = projects();
        for _ in 0..4 {
            tab(&mut dialog);
            assert!(matches!(dialog.focus, Field::Folder | Field::Worktree));
        }
    }

    #[test]
    fn the_shared_list_follows_the_focused_field() {
        let mut dialog = projects();
        assert_eq!(
            dialog.suggestions(),
            vec!["/work/web", "/Work/Docs", "/work/api"]
        );
        tab(&mut dialog);
        assert_eq!(dialog.focus, Field::Worktree);
        assert!(dialog.suggestions().is_empty());
        send(&mut dialog, press(KeyCode::Char(' ')));
        assert!(dialog.suggestions().is_empty());

        tab(&mut dialog);
        assert_eq!(
            dialog.suggestions(),
            vec!["main", "feat-a", "Feat-B", "docs"]
        );
        type_text(&mut dialog, "FEAT");
        assert_eq!(dialog.suggestions(), vec!["feat-a", "Feat-B"]);

        tab(&mut dialog);
        send(&mut dialog, ctrl('u'));
        assert_eq!(
            dialog.suggestions(),
            vec![
                "main",
                "feat-a",
                "Feat-B",
                "docs",
                "origin/main",
                "origin/feat-c"
            ]
        );
        type_text(&mut dialog, "feat");
        assert_eq!(
            dialog.suggestions(),
            vec!["feat-a", "Feat-B", "origin/feat-c"]
        );

        tab(&mut dialog);
        tab(&mut dialog);
        assert_eq!(dialog.focus, Field::Worktree);
        send(&mut dialog, press(KeyCode::BackTab));
        assert_eq!(dialog.suggestions().len(), 3);
    }

    #[test]
    fn each_field_keeps_its_own_highlight_when_focus_moves_away_and_back() {
        let mut dialog = name_focused();
        type_text(&mut dialog, "fe");
        down(&mut dialog);
        tab(&mut dialog);
        send(&mut dialog, press(KeyCode::BackTab));
        assert_eq!(
            (dialog.name.as_str(), dialog.highlight_index()),
            ("feat-a", Some(0))
        );
        up(&mut dialog);
        assert_eq!(dialog.name, "fe");
    }

    #[test]
    fn name_suggests_local_branches_only_and_base_suggests_remote_too() {
        let mut dialog = name_focused();
        type_text(&mut dialog, "origin");
        assert!(dialog.suggestions().is_empty());
        tab(&mut dialog);
        send(&mut dialog, ctrl('u'));
        type_text(&mut dialog, "origin");
        assert_eq!(dialog.suggestions(), vec!["origin/main", "origin/feat-c"]);
    }

    #[test]
    fn cycling_name_writes_each_branch_in_and_the_status_follows() {
        let mut dialog = name_focused();
        type_text(&mut dialog, "fe");
        assert_eq!(dialog.highlight_index(), None);
        down(&mut dialog);
        assert_eq!(dialog.name, "feat-a");
        assert_eq!(dialog.status_line().as_deref(), Some("existing branch"));
        down(&mut dialog);
        assert_eq!(dialog.name, "Feat-B");
        assert_eq!(dialog.suggestions(), vec!["feat-a", "Feat-B"]);
        down(&mut dialog);
        assert_eq!(dialog.name, "Feat-B");
        assert_eq!(
            enter(&mut dialog),
            Outcome::Submit(Submission {
                cwd: "/work/web".to_string(),
                worktree: Some(WorktreeRequest::Existing {
                    branch: "Feat-B".to_string(),
                    path: "/wt/feat-b".to_string()
                })
            })
        );
    }

    #[test]
    fn cycling_back_past_the_first_branch_restores_what_was_typed() {
        let mut dialog = name_focused();
        type_text(&mut dialog, "fe");
        down(&mut dialog);
        up(&mut dialog);
        assert_eq!((dialog.name.as_str(), dialog.name_cursor), ("fe", 2));
        assert_eq!(dialog.highlight_index(), None);
        assert_eq!(
            dialog.status_line().as_deref(),
            Some("new branch from branch-of:/work/web")
        );
    }

    #[test]
    fn enter_submits_what_is_in_the_field() {
        let mut dialog = name_focused();
        type_text(&mut dialog, "fe");
        assert_eq!(
            enter(&mut dialog),
            submit_new("/work/web", "fe", "branch-of:/work/web")
        );
    }

    #[test]
    fn down_on_an_empty_name_starts_at_the_first_branch() {
        let mut dialog = name_focused();
        down(&mut dialog);
        assert_eq!(dialog.name, "main");
        up(&mut dialog);
        assert_eq!(dialog.name, "");
    }

    #[test]
    fn cycling_base_counts_as_editing_and_stops_it_following_the_folder() {
        let mut dialog = base_focused_and_cleared();
        for _ in 0..6 {
            down(&mut dialog);
        }
        assert_eq!(dialog.base, "origin/feat-c");
        for _ in 0..3 {
            send(&mut dialog, press(KeyCode::BackTab));
        }
        down(&mut dialog);
        down(&mut dialog);
        assert_eq!(dialog.resolved_folder().as_deref(), Some("/Work/Docs"));
        assert_eq!(dialog.base, "origin/feat-c");
        tab(&mut dialog);
        tab(&mut dialog);
        tab(&mut dialog);
        assert_eq!(
            enter(&mut dialog),
            submit_new("/Work/Docs", "", "origin/feat-c")
        );
    }

    #[test]
    fn an_existing_branch_with_a_worktree_opens_it_without_a_base() {
        let mut dialog = name_focused();
        type_text(&mut dialog, "Feat-B");
        tab(&mut dialog);
        send(&mut dialog, ctrl('u'));
        assert_eq!(
            enter(&mut dialog),
            Outcome::Submit(Submission {
                cwd: "/work/web".to_string(),
                worktree: Some(WorktreeRequest::Existing {
                    branch: "Feat-B".to_string(),
                    path: "/wt/feat-b".to_string()
                })
            })
        );
    }

    #[test]
    fn an_existing_branch_without_a_worktree_is_checked_out() {
        let mut dialog = name_focused();
        type_text(&mut dialog, "docs");
        assert_eq!(
            enter(&mut dialog),
            Outcome::Submit(Submission {
                cwd: "/work/web".to_string(),
                worktree: Some(WorktreeRequest::Checkout {
                    branch: "docs".to_string()
                })
            })
        );
    }

    #[test]
    fn checking_out_needs_worktree_checkout_to_be_configured() {
        let mut dialog = name_focused();
        dialog.can_checkout = false;
        type_text(&mut dialog, "docs");
        assert_eq!(enter(&mut dialog), Outcome::Pending);
        assert_eq!(
            dialog.error.as_deref(),
            Some("no worktree.checkout configured")
        );
        assert_eq!(dialog.focus, Field::Name);
        send(&mut dialog, press(KeyCode::Left));
        assert_eq!(dialog.error, None);
    }

    #[test]
    fn repo_data_reloads_with_the_folder_so_statuses_follow_it() {
        let load = |folder: &str| -> Result<RepoInfo> {
            let mut info = repo_of(folder)?;
            if folder == "/Work/Docs" {
                info.local = vec!["only-here".to_string()];
            }
            Ok(info)
        };
        let mut dialog = name_focused();
        type_text(&mut dialog, "docs");
        assert_eq!(dialog.name_status(), NameStatus::ExistingWithoutWorktree);
        for _ in 0..2 {
            dialog.handle_key(press(KeyCode::BackTab), &load);
        }
        dialog.handle_key(press(KeyCode::Down), &load);
        dialog.handle_key(press(KeyCode::Down), &load);
        assert_eq!(dialog.name_status(), NameStatus::New);
    }

    #[test]
    fn a_prefilled_base_does_not_filter_the_list() {
        let mut dialog = name_focused();
        tab(&mut dialog);
        assert_eq!(dialog.base, "branch-of:/work/web");
        assert_eq!(dialog.suggestions().len(), 6);
        assert_eq!(dialog.highlight_index(), None);
    }

    #[test]
    fn cycling_a_prefilled_base_and_going_back_restores_the_prefill() {
        let mut dialog = name_focused();
        tab(&mut dialog);
        down(&mut dialog);
        assert_eq!(dialog.base, "main");
        down(&mut dialog);
        assert_eq!(dialog.base, "feat-a");
        up(&mut dialog);
        up(&mut dialog);
        assert_eq!(dialog.base, "branch-of:/work/web");
        assert_eq!(dialog.base_cursor, dialog.base.chars().count());
        assert_eq!(dialog.highlight_index(), None);
        up(&mut dialog);
        assert_eq!(dialog.base, "branch-of:/work/web");
    }

    #[test]
    fn the_restore_value_is_the_text_when_cycling_started() {
        let mut dialog = name_focused();
        type_text(&mut dialog, "fe");
        down(&mut dialog);
        down(&mut dialog);
        type_text(&mut dialog, "x");
        up(&mut dialog);
        assert_eq!(dialog.name, "Feat-Bx");
        down(&mut dialog);
        assert_eq!(dialog.name, "Feat-Bx");
        type_text(&mut dialog, "y");
        assert_eq!(dialog.name, "Feat-Bxy");
    }

    #[test]
    fn the_prefill_refreshes_with_the_folder_and_keeps_the_list_unfiltered() {
        let mut dialog = with_worktree_on();
        assert_eq!(dialog.base, "branch-of:/work/web");
        for _ in 0..1 {
            send(&mut dialog, press(KeyCode::BackTab));
        }
        down(&mut dialog);
        down(&mut dialog);
        assert_eq!(dialog.base, "branch-of:/Work/Docs");
        tab(&mut dialog);
        tab(&mut dialog);
        tab(&mut dialog);
        assert_eq!(dialog.suggestions().len(), 6);
        down(&mut dialog);
        up(&mut dialog);
        assert_eq!(dialog.base, "branch-of:/Work/Docs");
    }
}
