use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders};

pub fn render(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" [1] Sessions ")
        .border_style(Style::default().fg(Color::Rgb(0xD9, 0x77, 0x57)));
    frame.render_widget(block, area);
}
