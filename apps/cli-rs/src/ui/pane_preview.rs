use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::AppState;

const PRIMARY: Color = Color::Rgb(0xD9, 0x77, 0x57);
const UNFOCUSED: Color = Color::Rgb(0x66, 0x66, 0x66);

pub fn render(frame: &mut Frame, area: Rect, state: &AppState, focused: bool) {
    let border_color = if focused { PRIMARY } else { UNFOCUSED };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" [0] Preview ")
        .border_style(Style::default().fg(border_color));

    let paragraph = Paragraph::new(state.preview_content.as_str()).block(block);
    frame.render_widget(paragraph, area);
}
