use ratatui::prelude::*;

use crate::app::AppState;

pub mod session_list;
pub mod pane_preview;

pub fn render(frame: &mut Frame, _state: &AppState) {
    let chunks = Layout::horizontal([
        Constraint::Length(40),
        Constraint::Min(1),
    ])
    .split(frame.area());

    session_list::render(frame, chunks[0]);
    pane_preview::render(frame, chunks[1]);
}
