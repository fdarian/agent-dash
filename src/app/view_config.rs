use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::sessions::view::{ActivityWindow, Grouping, StatusFilter, ViewConfig};
use crate::ui::list_nav::{nav_action, step, NavAction, NavMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Status,
    Grouping,
    Activity,
}

impl Row {
    pub const ALL: [Row; 3] = [Row::Status, Row::Grouping, Row::Activity];

    pub fn label(self) -> &'static str {
        match self {
            Row::Status => "Status",
            Row::Grouping => "Grouping",
            Row::Activity => "Last activity",
        }
    }

    pub fn index(self) -> usize {
        Row::ALL
            .iter()
            .position(|row| *row == self)
            .expect("Row::ALL lists every row")
    }

    pub fn options(self) -> Vec<&'static str> {
        match self {
            Row::Status => StatusFilter::ALL.iter().map(|v| v.label()).collect(),
            Row::Grouping => Grouping::ALL.iter().map(|v| v.label()).collect(),
            Row::Activity => ActivityWindow::ALL.iter().map(|v| v.label()).collect(),
        }
    }

    pub fn current_index(self, view: &ViewConfig) -> usize {
        let found = match self {
            Row::Status => StatusFilter::ALL.iter().position(|v| *v == view.status),
            Row::Grouping => Grouping::ALL.iter().position(|v| *v == view.grouping),
            Row::Activity => ActivityWindow::ALL.iter().position(|v| *v == view.activity),
        };
        found.expect("every variant is listed in its ALL array")
    }

    pub fn value_label(self, view: &ViewConfig) -> &'static str {
        self.options()[self.current_index(view)]
    }

    /// Returns whether the config changed.
    fn apply(self, view: &mut ViewConfig, option: usize) -> bool {
        let before = *view;
        match self {
            Row::Status => view.status = StatusFilter::ALL[option],
            Row::Grouping => view.grouping = Grouping::ALL[option],
            Row::Activity => view.activity = ActivityWindow::ALL[option],
        }
        *view != before
    }
}

/// `S` overlay: the three setting rows, or the option selector opened from one of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewConfigOverlay {
    Rows { cursor: Row },
    Selector { row: Row, cursor: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Unchanged,
    /// The view config was edited; the list needs rebuilding and the config saving.
    Changed,
    Closed,
}

impl ViewConfigOverlay {
    pub fn open() -> Self {
        ViewConfigOverlay::Rows {
            cursor: Row::Status,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, view: &mut ViewConfig) -> Outcome {
        match *self {
            ViewConfigOverlay::Rows { cursor } => {
                if key.modifiers == KeyModifiers::NONE {
                    if let KeyCode::Char(c @ '1'..='3') = key.code {
                        let row = Row::ALL[c as usize - '1' as usize];
                        *self = Self::select(row, view);
                        return Outcome::Unchanged;
                    }
                }
                match nav_action(key, NavMode::List) {
                    Some(NavAction::Close) => Outcome::Closed,
                    Some(NavAction::Select) => {
                        *self = Self::select(cursor, view);
                        Outcome::Unchanged
                    }
                    Some(action @ (NavAction::Next | NavAction::Prev)) => {
                        let next = step(cursor.index(), Row::ALL.len(), action);
                        *self = ViewConfigOverlay::Rows {
                            cursor: Row::ALL[next],
                        };
                        Outcome::Unchanged
                    }
                    None => Outcome::Unchanged,
                }
            }
            ViewConfigOverlay::Selector { row, cursor } => match nav_action(key, NavMode::List) {
                Some(NavAction::Close) => {
                    *self = ViewConfigOverlay::Rows { cursor: row };
                    Outcome::Unchanged
                }
                Some(NavAction::Select) => {
                    *self = ViewConfigOverlay::Rows { cursor: row };
                    if row.apply(view, cursor) {
                        Outcome::Changed
                    } else {
                        Outcome::Unchanged
                    }
                }
                Some(action @ (NavAction::Next | NavAction::Prev)) => {
                    let next = step(cursor, row.options().len(), action);
                    *self = ViewConfigOverlay::Selector { row, cursor: next };
                    Outcome::Unchanged
                }
                None => Outcome::Unchanged,
            },
        }
    }

    fn select(row: Row, view: &ViewConfig) -> Self {
        ViewConfigOverlay::Selector {
            row,
            cursor: row.current_index(view),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ch(c: char) -> KeyEvent {
        press(KeyCode::Char(c))
    }

    fn send(overlay: &mut ViewConfigOverlay, view: &mut ViewConfig, key: KeyEvent) -> Outcome {
        overlay.handle_key(key, view)
    }

    #[test]
    fn opens_on_the_status_row() {
        assert_eq!(
            ViewConfigOverlay::open(),
            ViewConfigOverlay::Rows {
                cursor: Row::Status
            }
        );
    }

    #[test]
    fn digits_open_the_matching_selector_on_the_current_value() {
        let mut view = ViewConfig::default();
        for (digit, row, cursor) in [
            ('1', Row::Status, 1),
            ('2', Row::Grouping, 0),
            ('3', Row::Activity, 2),
        ] {
            let mut overlay = ViewConfigOverlay::open();
            let outcome = send(&mut overlay, &mut view, ch(digit));
            assert_eq!(outcome, Outcome::Unchanged);
            assert_eq!(overlay, ViewConfigOverlay::Selector { row, cursor });
        }
    }

    #[test]
    fn rows_cursor_moves_and_clamps() {
        let mut view = ViewConfig::default();
        let mut overlay = ViewConfigOverlay::open();
        send(&mut overlay, &mut view, ch('k'));
        assert_eq!(
            overlay,
            ViewConfigOverlay::Rows {
                cursor: Row::Status
            }
        );
        send(&mut overlay, &mut view, ch('j'));
        send(&mut overlay, &mut view, ch('j'));
        send(&mut overlay, &mut view, press(KeyCode::Down));
        assert_eq!(
            overlay,
            ViewConfigOverlay::Rows {
                cursor: Row::Activity
            }
        );
    }

    #[test]
    fn enter_on_a_row_opens_its_selector() {
        let mut view = ViewConfig::default();
        let mut overlay = ViewConfigOverlay::open();
        send(&mut overlay, &mut view, ch('j'));
        send(&mut overlay, &mut view, press(KeyCode::Enter));
        assert_eq!(
            overlay,
            ViewConfigOverlay::Selector {
                row: Row::Grouping,
                cursor: 0
            }
        );
    }

    #[test]
    fn enter_in_a_selector_applies_and_returns_to_rows() {
        let mut view = ViewConfig::default();
        let mut overlay = ViewConfigOverlay::open();
        send(&mut overlay, &mut view, ch('3'));
        send(&mut overlay, &mut view, ch('j'));
        send(&mut overlay, &mut view, ch('j'));
        let outcome = send(&mut overlay, &mut view, press(KeyCode::Enter));
        assert_eq!(outcome, Outcome::Changed);
        assert_eq!(view.activity, ActivityWindow::All);
        assert_eq!(
            overlay,
            ViewConfigOverlay::Rows {
                cursor: Row::Activity
            }
        );
    }

    #[test]
    fn applying_the_current_value_is_not_a_change() {
        let mut view = ViewConfig::default();
        let mut overlay = ViewConfigOverlay::open();
        send(&mut overlay, &mut view, ch('2'));
        let outcome = send(&mut overlay, &mut view, press(KeyCode::Enter));
        assert_eq!(outcome, Outcome::Unchanged);
        assert_eq!(view, ViewConfig::default());
    }

    #[test]
    fn selector_moving_does_not_touch_the_config_until_enter() {
        let mut view = ViewConfig::default();
        let mut overlay = ViewConfigOverlay::open();
        send(&mut overlay, &mut view, ch('2'));
        send(&mut overlay, &mut view, press(KeyCode::Down));
        assert_eq!(view, ViewConfig::default());
        let outcome = send(&mut overlay, &mut view, press(KeyCode::Enter));
        assert_eq!(outcome, Outcome::Changed);
        assert_eq!(view.grouping, Grouping::Folder);
    }

    #[test]
    fn esc_backs_out_one_level_at_a_time() {
        let mut view = ViewConfig::default();
        let mut overlay = ViewConfigOverlay::open();
        send(&mut overlay, &mut view, ch('2'));
        send(&mut overlay, &mut view, press(KeyCode::Down));
        let outcome = send(&mut overlay, &mut view, press(KeyCode::Esc));
        assert_eq!(outcome, Outcome::Unchanged);
        assert_eq!(
            overlay,
            ViewConfigOverlay::Rows {
                cursor: Row::Grouping
            }
        );
        assert_eq!(view, ViewConfig::default());
        let outcome = send(&mut overlay, &mut view, press(KeyCode::Esc));
        assert_eq!(outcome, Outcome::Closed);
    }

    #[test]
    fn digits_do_nothing_inside_a_selector() {
        let mut view = ViewConfig::default();
        let mut overlay = ViewConfigOverlay::open();
        send(&mut overlay, &mut view, ch('1'));
        let before = overlay;
        send(&mut overlay, &mut view, ch('3'));
        assert_eq!(overlay, before);
    }

    #[test]
    fn row_labels_show_the_current_values() {
        let view = ViewConfig::default();
        assert_eq!(Row::Status.value_label(&view), "Active");
        assert_eq!(Row::Grouping.value_label(&view), "State");
        assert_eq!(Row::Activity.value_label(&view), "7d");
    }
}
