use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::AppState;

const PRIMARY: Color = Color::Rgb(0xD9, 0x77, 0x57);
const UNFOCUSED: Color = Color::Rgb(0x66, 0x66, 0x66);

pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState, focused: bool) {
    let border_color = if focused { PRIMARY } else { UNFOCUSED };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" [0] Preview ")
        .border_style(Style::default().fg(border_color));

    let inner_area = block.inner(area);

    if state.preview_content.is_empty() {
        frame.render_widget(block, area);
        return;
    }

    let text = ansi_to_tui::IntoText::into_text(&state.preview_content).unwrap_or_default();
    let content_height = text.lines.len() as u16;
    state.preview_content_height = content_height;

    if state.preview_is_sticky_bottom {
        let visible_height = inner_area.height;
        state.preview_scroll_offset = content_height.saturating_sub(visible_height);
    }

    let paragraph = Paragraph::new(text)
        .block(block)
        .scroll((state.preview_scroll_offset, 0));

    frame.render_widget(paragraph, area);
}
