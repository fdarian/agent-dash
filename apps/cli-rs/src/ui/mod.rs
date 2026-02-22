use ratatui::prelude::*;

use crate::app::{AppState, Focus};

pub mod pane_preview;
pub mod session_list;

pub fn render(frame: &mut Frame, state: &mut AppState) {
    match state.focus {
        Focus::Sessions => {
            let chunks = Layout::horizontal([Constraint::Length(40), Constraint::Min(1)])
                .split(frame.area());
            session_list::render(frame, chunks[0], state, true);
            pane_preview::render(frame, chunks[1], state, false);
        }
        Focus::Preview => {
            pane_preview::render(frame, frame.area(), state, true);
        }
    }
}
