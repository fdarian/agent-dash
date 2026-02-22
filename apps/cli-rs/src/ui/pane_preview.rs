use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders};

pub fn render(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" [0] Preview ")
        .border_style(Style::default().fg(Color::Rgb(0x66, 0x66, 0x66)));
    frame.render_widget(block, area);
}
