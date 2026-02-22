use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::AppState;

const PRIMARY: Color = Color::Rgb(0xD9, 0x77, 0x57);

pub fn render(frame: &mut Frame, state: &AppState) {
    let area = frame.area();

    let width = area.width * 40 / 100;
    let height = 5;
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let popup_area = Rect::new(x, y, width, height);

    let bg_color = Color::Rgb(state.terminal_bg.0, state.terminal_bg.1, state.terminal_bg.2);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Confirm ")
        .border_style(Style::default().fg(PRIMARY))
        .style(Style::default().bg(bg_color));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if let Some(ref target) = state.pending_confirm_target {
        let message = Line::from(format!("Close session {}?", target))
            .fg(Color::Rgb(0xCC, 0xCC, 0xCC));
        let hint = Line::from("[Enter] Confirm  [Esc] Cancel")
            .fg(Color::Rgb(0x66, 0x66, 0x66));

        let msg_area = Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), 1);
        let hint_area = Rect::new(inner.x + 1, inner.y + 1, inner.width.saturating_sub(2), 1);

        frame.render_widget(
            Paragraph::new(message).alignment(Alignment::Center),
            msg_area,
        );
        frame.render_widget(
            Paragraph::new(hint).alignment(Alignment::Center),
            hint_area,
        );
    }
}
