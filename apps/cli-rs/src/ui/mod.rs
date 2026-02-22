use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::{AppState, Focus};

pub mod confirm_dialog;
pub mod help_overlay;
pub mod keybinds;
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

    // Overlays rendered on top of main layout
    if state.pending_confirm_target.is_some() {
        confirm_dialog::render(frame, state);
    }
    if state.show_help {
        help_overlay::render(frame, state);
    }

    if let Some(ref msg) = state.toast_message {
        let area = frame.area();
        let toast_width = (msg.len() + 2) as u16;
        let toast_area = Rect::new(
            area.width.saturating_sub(toast_width + 1),
            1,
            toast_width,
            1,
        );
        let toast = Paragraph::new(format!(" {} ", msg))
            .style(Style::default().fg(Color::Black).bg(Color::Rgb(0xD9, 0x77, 0x57)));
        frame.render_widget(toast, toast_area);
    }
}
