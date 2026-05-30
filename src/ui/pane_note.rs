use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::AppState;

/// Renders the note for the currently selected session or group as a read-only
/// pane. Only called when the selected item has a non-empty note (editing still
/// happens in $EDITOR via the `n` key).
pub fn render(frame: &mut Frame, area: Rect, state: &AppState, content: &str) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Note ")
        .border_style(Style::default().fg(state.theme.border_unfocused));
    let paragraph = Paragraph::new(content)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}
