use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavAction {
    Next,
    Prev,
    Select,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavMode {
    /// Plain keys are free, so j/k navigate too.
    List,
    /// A text input has focus and owns printable keys, so only arrows and Ctrl+n/Ctrl+p navigate.
    TextInput,
}

pub fn nav_action(key: KeyEvent, mode: NavMode) -> Option<NavAction> {
    let plain = key.modifiers.is_empty();
    let ctrl = key.modifiers == KeyModifiers::CONTROL;
    match key.code {
        KeyCode::Down => Some(NavAction::Next),
        KeyCode::Up => Some(NavAction::Prev),
        KeyCode::Char('n') if ctrl => Some(NavAction::Next),
        KeyCode::Char('p') if ctrl => Some(NavAction::Prev),
        KeyCode::Char('j') if plain && mode == NavMode::List => Some(NavAction::Next),
        KeyCode::Char('k') if plain && mode == NavMode::List => Some(NavAction::Prev),
        KeyCode::Enter => Some(NavAction::Select),
        KeyCode::Esc => Some(NavAction::Close),
        _ => None,
    }
}

/// Moves a cursor one step, clamped to `0..len`.
pub fn step(index: usize, len: usize, action: NavAction) -> usize {
    let last = len.saturating_sub(1);
    match action {
        NavAction::Next => (index + 1).min(last),
        NavAction::Prev => index.saturating_sub(1),
        NavAction::Select | NavAction::Close => index.min(last),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn plain(code: KeyCode) -> KeyEvent {
        key(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        key(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn list_mode_maps_every_navigation_key() {
        let cases = [
            (plain(KeyCode::Char('j')), NavAction::Next),
            (plain(KeyCode::Down), NavAction::Next),
            (ctrl('n'), NavAction::Next),
            (plain(KeyCode::Char('k')), NavAction::Prev),
            (plain(KeyCode::Up), NavAction::Prev),
            (ctrl('p'), NavAction::Prev),
            (plain(KeyCode::Enter), NavAction::Select),
            (plain(KeyCode::Esc), NavAction::Close),
        ];
        for (event, expected) in cases {
            assert_eq!(
                nav_action(event, NavMode::List),
                Some(expected),
                "{event:?}"
            );
        }
    }

    #[test]
    fn text_input_mode_leaves_j_and_k_to_the_input() {
        assert_eq!(
            nav_action(plain(KeyCode::Char('j')), NavMode::TextInput),
            None
        );
        assert_eq!(
            nav_action(plain(KeyCode::Char('k')), NavMode::TextInput),
            None
        );
    }

    #[test]
    fn text_input_mode_still_navigates_with_arrows_and_ctrl() {
        let cases = [
            (plain(KeyCode::Down), NavAction::Next),
            (plain(KeyCode::Up), NavAction::Prev),
            (ctrl('n'), NavAction::Next),
            (ctrl('p'), NavAction::Prev),
            (plain(KeyCode::Enter), NavAction::Select),
            (plain(KeyCode::Esc), NavAction::Close),
        ];
        for (event, expected) in cases {
            assert_eq!(
                nav_action(event, NavMode::TextInput),
                Some(expected),
                "{event:?}"
            );
        }
    }

    #[test]
    fn modified_letters_and_other_keys_are_not_navigation() {
        for mode in [NavMode::List, NavMode::TextInput] {
            assert_eq!(nav_action(ctrl('j'), mode), None);
            assert_eq!(nav_action(ctrl('k'), mode), None);
            assert_eq!(nav_action(plain(KeyCode::Char('n')), mode), None);
            assert_eq!(nav_action(plain(KeyCode::Char('p')), mode), None);
            assert_eq!(nav_action(plain(KeyCode::Char('x')), mode), None);
        }
        assert_eq!(
            nav_action(key(KeyCode::Char('j'), KeyModifiers::ALT), NavMode::List),
            None
        );
    }

    #[test]
    fn step_clamps_at_both_ends() {
        assert_eq!(step(0, 3, NavAction::Prev), 0);
        assert_eq!(step(2, 3, NavAction::Next), 2);
        assert_eq!(step(1, 3, NavAction::Next), 2);
        assert_eq!(step(1, 3, NavAction::Prev), 0);
        assert_eq!(step(0, 0, NavAction::Next), 0);
    }
}
