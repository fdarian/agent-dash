use std::collections::HashSet;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::keys::apply_text_input;
use crate::fuzzy::fuzzy_rank;
use crate::sessions::import::{ImportCandidate, ImportSource, ScanOutcome};
use crate::sessions::{basename, display_title};
use crate::ui::list_nav::{nav_action, step, NavAction, NavMode};

/// State of the `agent-dash import` picker. Nothing is selected until the user picks.
pub struct ImportPicker {
    pub candidates: Vec<ImportCandidate>,
    pub skipped: usize,
    pub selected: HashSet<String>,
    /// `None` shows both sources.
    pub source: Option<ImportSource>,
    pub query: String,
    pub query_cursor: usize,
    /// The query input has focus, so printable keys edit it instead of acting on the list.
    pub editing_query: bool,
    /// Index into `visible`.
    pub cursor: usize,
    /// Indices into `candidates` that match the query, in display order.
    pub visible: Vec<usize>,
}

/// All, then CLI, then Desktop.
pub const SOURCE_TABS: [Option<ImportSource>; 3] =
    [None, Some(ImportSource::Cli), Some(ImportSource::Desktop)];

fn next_source(current: Option<ImportSource>) -> Option<ImportSource> {
    let index = SOURCE_TABS.iter().position(|tab| *tab == current);
    SOURCE_TABS[(index.expect("every source is a tab") + 1) % SOURCE_TABS.len()]
}

fn prev_source(current: Option<ImportSource>) -> Option<ImportSource> {
    let index = SOURCE_TABS.iter().position(|tab| *tab == current);
    SOURCE_TABS[(index.expect("every source is a tab") + SOURCE_TABS.len() - 1) % SOURCE_TABS.len()]
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Pending,
    Closed,
    Import(Vec<ImportCandidate>),
}

impl ImportPicker {
    pub fn new(outcome: ScanOutcome) -> ImportPicker {
        let visible = (0..outcome.candidates.len()).collect();
        ImportPicker {
            candidates: outcome.candidates,
            skipped: outcome.skipped,
            selected: HashSet::new(),
            source: None,
            query: String::new(),
            query_cursor: 0,
            editing_query: false,
            cursor: 0,
            visible,
        }
    }

    pub fn is_selected(&self, candidate: &ImportCandidate) -> bool {
        self.selected.contains(&candidate.session_id)
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Tab if key.modifiers == KeyModifiers::NONE => {
                self.set_source(next_source(self.source));
                return Outcome::Pending;
            }
            KeyCode::BackTab => {
                self.set_source(prev_source(self.source));
                return Outcome::Pending;
            }
            _ => {}
        }
        if self.editing_query {
            return self.handle_query_key(key);
        }
        if key.modifiers == KeyModifiers::NONE {
            match key.code {
                KeyCode::Char(' ') => {
                    self.toggle_current();
                    return Outcome::Pending;
                }
                KeyCode::Char('a') => {
                    self.select_all_visible();
                    return Outcome::Pending;
                }
                KeyCode::Char('n') => {
                    self.selected.clear();
                    return Outcome::Pending;
                }
                KeyCode::Char('/') => {
                    self.editing_query = true;
                    return Outcome::Pending;
                }
                KeyCode::Char('q') => return Outcome::Closed,
                _ => {}
            }
        }
        match nav_action(key, NavMode::List) {
            Some(NavAction::Close) if !self.query.is_empty() => {
                self.clear_query();
                Outcome::Pending
            }
            Some(NavAction::Close) => Outcome::Closed,
            Some(NavAction::Select) => self.import_selected(),
            Some(action @ (NavAction::Next | NavAction::Prev)) => {
                self.cursor = step(self.cursor, self.visible.len(), action);
                Outcome::Pending
            }
            None => Outcome::Pending,
        }
    }

    fn handle_query_key(&mut self, key: KeyEvent) -> Outcome {
        match nav_action(key, NavMode::TextInput) {
            Some(NavAction::Close) => self.clear_query(),
            Some(NavAction::Select) => self.editing_query = false,
            Some(action @ (NavAction::Next | NavAction::Prev)) => {
                self.cursor = step(self.cursor, self.visible.len(), action);
            }
            None => {
                if apply_text_input(&mut self.query, &mut self.query_cursor, key) {
                    self.refresh_visible();
                    self.cursor = 0;
                }
            }
        }
        Outcome::Pending
    }

    fn clear_query(&mut self) {
        self.editing_query = false;
        self.query.clear();
        self.query_cursor = 0;
        self.refresh_visible();
        self.cursor = 0;
    }

    fn set_source(&mut self, source: Option<ImportSource>) {
        self.source = source;
        self.refresh_visible();
        self.cursor = 0;
    }

    fn refresh_visible(&mut self) {
        let query = self.query.trim();
        let in_source = |candidate: &ImportCandidate| {
            self.source.is_none_or(|source| candidate.source == source)
        };
        self.visible = if query.is_empty() {
            (0..self.candidates.len())
                .filter(|index| in_source(&self.candidates[*index]))
                .collect()
        } else {
            let haystacks = self
                .candidates
                .iter()
                .enumerate()
                .filter(|(_, candidate)| in_source(candidate))
                .map(|(i, candidate)| {
                    let haystack = format!(
                        "{}/{}",
                        basename(&candidate.cwd),
                        display_title(&candidate.title, &candidate.session_id)
                    );
                    (i, haystack)
                });
            fuzzy_rank(haystacks, query)
        };
    }

    fn toggle_current(&mut self) {
        let Some(index) = self.visible.get(self.cursor) else {
            return;
        };
        let session_id = &self.candidates[*index].session_id;
        if !self.selected.remove(session_id) {
            self.selected.insert(session_id.clone());
        }
    }

    fn select_all_visible(&mut self) {
        let candidates = &self.candidates;
        self.selected.extend(
            self.visible
                .iter()
                .map(|index| candidates[*index].session_id.clone()),
        );
    }

    fn import_selected(&self) -> Outcome {
        let chosen: Vec<ImportCandidate> = self
            .candidates
            .iter()
            .filter(|candidate| self.is_selected(candidate))
            .cloned()
            .collect();
        if chosen.is_empty() {
            Outcome::Pending
        } else {
            Outcome::Import(chosen)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::import::ImportSource;

    fn candidate(id: &str, title: &str, cwd: &str) -> ImportCandidate {
        ImportCandidate {
            session_id: id.to_string(),
            source: ImportSource::Cli,
            title: title.to_string(),
            cwd: cwd.to_string(),
            last_activity: 1,
        }
    }

    fn desktop(mut candidate: ImportCandidate) -> ImportCandidate {
        candidate.source = ImportSource::Desktop;
        candidate
    }

    fn picker() -> ImportPicker {
        ImportPicker::new(ScanOutcome {
            candidates: vec![
                candidate("a", "fix login", "/work/app"),
                candidate("b", "write docs", "/work/site"),
                candidate("c", "old idea", "/work/app"),
                desktop(candidate("d", "docs again", "/work/site")),
            ],
            skipped: 0,
        })
    }

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ch(c: char) -> KeyEvent {
        let modifiers = if c.is_uppercase() {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        };
        KeyEvent::new(KeyCode::Char(c), modifiers)
    }

    fn type_filter(list: &mut ImportPicker, text: &str) {
        list.handle_key(ch('/'));
        for c in text.chars() {
            list.handle_key(ch(c));
        }
    }

    fn selected_ids(list: &ImportPicker) -> Vec<String> {
        let mut ids: Vec<String> = list.selected.iter().cloned().collect();
        ids.sort();
        ids
    }

    #[test]
    fn nothing_is_selected_by_default() {
        assert!(picker().selected.is_empty());
    }

    #[test]
    fn space_toggles_the_row_under_the_cursor() {
        let mut list = picker();
        list.handle_key(ch('j'));
        list.handle_key(ch(' '));
        assert_eq!(selected_ids(&list), vec!["b"]);
        list.handle_key(ch(' '));
        assert!(list.selected.is_empty());
    }

    #[test]
    fn a_selects_every_row_and_n_deselects_all() {
        let mut list = picker();
        list.handle_key(ch('a'));
        assert_eq!(selected_ids(&list), vec!["a", "b", "c", "d"]);
        list.handle_key(ch('a'));
        assert_eq!(selected_ids(&list), vec!["a", "b", "c", "d"]);
        list.handle_key(ch('n'));
        assert!(list.selected.is_empty());
    }

    #[test]
    fn a_only_selects_rows_the_filter_shows() {
        let mut list = picker();
        type_filter(&mut list, "docs");
        assert_eq!(list.visible, vec![1, 3]);
        list.handle_key(press(KeyCode::Enter));
        list.handle_key(ch('a'));
        assert_eq!(selected_ids(&list), vec!["b", "d"]);
    }

    #[test]
    fn n_deselects_rows_the_filter_hides_too() {
        let mut list = picker();
        list.handle_key(ch('a'));
        type_filter(&mut list, "docs");
        list.handle_key(press(KeyCode::Enter));
        list.handle_key(ch('n'));
        assert!(list.selected.is_empty());
    }

    #[test]
    fn slash_filters_fuzzily_on_folder_and_title() {
        let mut list = picker();
        type_filter(&mut list, "site");
        assert!(list.editing_query);
        assert_eq!(list.visible, vec![1, 3]);
    }

    #[test]
    fn typing_in_the_filter_does_not_trigger_list_keys() {
        let mut list = picker();
        list.handle_key(ch('/'));
        for c in " anq".chars() {
            assert_eq!(list.handle_key(ch(c)), Outcome::Pending);
        }
        assert!(list.selected.is_empty());
        assert_eq!(list.query, " anq");
    }

    #[test]
    fn enter_in_the_filter_keeps_it_and_returns_to_the_list() {
        let mut list = picker();
        type_filter(&mut list, "a");
        list.handle_key(press(KeyCode::Enter));
        assert!(!list.editing_query);
        assert_eq!(list.query, "a");
    }

    #[test]
    fn esc_in_the_filter_clears_it_and_esc_again_closes() {
        let mut list = picker();
        type_filter(&mut list, "d");
        assert_eq!(list.handle_key(press(KeyCode::Esc)), Outcome::Pending);
        assert!(list.query.is_empty() && !list.editing_query);
        assert_eq!(list.visible.len(), 4);
        assert_eq!(list.handle_key(press(KeyCode::Esc)), Outcome::Closed);
    }

    #[test]
    fn esc_with_a_kept_filter_clears_it_before_closing() {
        let mut list = picker();
        type_filter(&mut list, "d");
        list.handle_key(press(KeyCode::Enter));
        assert_eq!(list.handle_key(press(KeyCode::Esc)), Outcome::Pending);
        assert_eq!(list.visible.len(), 4);
        assert_eq!(list.handle_key(press(KeyCode::Esc)), Outcome::Closed);
    }

    fn tab() -> KeyEvent {
        press(KeyCode::Tab)
    }

    fn back_tab() -> KeyEvent {
        KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)
    }

    #[test]
    fn tab_cycles_all_cli_desktop_and_shift_tab_goes_back() {
        let mut list = picker();
        assert_eq!(list.source, None);
        list.handle_key(tab());
        assert_eq!(list.source, Some(ImportSource::Cli));
        assert_eq!(list.visible, vec![0, 1, 2]);
        list.handle_key(tab());
        assert_eq!(list.source, Some(ImportSource::Desktop));
        assert_eq!(list.visible, vec![3]);
        list.handle_key(tab());
        assert_eq!(list.source, None);
        assert_eq!(list.visible.len(), 4);
        list.handle_key(back_tab());
        assert_eq!(list.source, Some(ImportSource::Desktop));
        list.handle_key(back_tab());
        assert_eq!(list.source, Some(ImportSource::Cli));
    }

    #[test]
    fn source_filter_combines_with_the_text_filter_and_a_respects_both() {
        let mut list = picker();
        type_filter(&mut list, "docs");
        list.handle_key(press(KeyCode::Enter));
        assert_eq!(list.visible, vec![1, 3]);
        list.handle_key(tab());
        assert_eq!(list.visible, vec![1]);
        list.handle_key(ch('a'));
        assert_eq!(selected_ids(&list), vec!["b"]);
    }

    #[test]
    fn text_filter_edits_keep_the_source_filter() {
        let mut list = picker();
        list.handle_key(tab());
        list.handle_key(tab());
        type_filter(&mut list, "docs");
        assert_eq!(list.visible, vec![3]);
        list.handle_key(press(KeyCode::Esc));
        assert_eq!(list.visible, vec![3]);
    }

    #[test]
    fn selection_survives_source_changes_and_enter_imports_all_of_it() {
        let mut list = picker();
        list.handle_key(ch(' '));
        list.handle_key(tab());
        list.handle_key(tab());
        list.handle_key(ch(' '));
        list.handle_key(tab());
        assert_eq!(selected_ids(&list), vec!["a", "d"]);
        list.handle_key(tab());
        let Outcome::Import(chosen) = list.handle_key(press(KeyCode::Enter)) else {
            panic!("expected an import");
        };
        let ids: Vec<&str> = chosen.iter().map(|c| c.session_id.as_str()).collect();
        assert_eq!(ids, vec!["a", "d"]);
    }

    #[test]
    fn tab_works_while_editing_the_filter() {
        let mut list = picker();
        list.handle_key(ch('/'));
        list.handle_key(tab());
        assert!(list.editing_query);
        assert_eq!(list.source, Some(ImportSource::Cli));
        assert!(list.query.is_empty());
    }

    #[test]
    fn q_closes_without_importing() {
        let mut list = picker();
        list.handle_key(ch('a'));
        assert_eq!(list.handle_key(ch('q')), Outcome::Closed);
    }

    #[test]
    fn enter_imports_the_selection_including_rows_hidden_by_the_filter() {
        let mut list = picker();
        list.handle_key(ch(' '));
        list.handle_key(ch('j'));
        list.handle_key(ch(' '));
        type_filter(&mut list, "idea");
        list.handle_key(press(KeyCode::Enter));
        let Outcome::Import(chosen) = list.handle_key(press(KeyCode::Enter)) else {
            panic!("expected an import");
        };
        let ids: Vec<&str> = chosen.iter().map(|c| c.session_id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"]);
    }

    #[test]
    fn enter_with_nothing_selected_stays_open() {
        let mut list = picker();
        assert_eq!(list.handle_key(press(KeyCode::Enter)), Outcome::Pending);
    }

    #[test]
    fn navigation_keys_move_and_clamp_the_cursor() {
        let mut list = picker();
        list.handle_key(ch('k'));
        assert_eq!(list.cursor, 0);
        list.handle_key(press(KeyCode::Down));
        list.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL));
        list.handle_key(ch('j'));
        list.handle_key(ch('j'));
        assert_eq!(list.cursor, 3);
        list.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
        assert_eq!(list.cursor, 2);
        assert!(list.selected.is_empty());
    }

    #[test]
    fn empty_scan_result_is_harmless() {
        let mut list = ImportPicker::new(ScanOutcome::default());
        for c in " ajn".chars() {
            list.handle_key(ch(c));
        }
        assert_eq!(list.handle_key(press(KeyCode::Enter)), Outcome::Pending);
    }
}
