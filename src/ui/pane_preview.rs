use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};

use crate::app::AppState;

const PRIMARY: Color = Color::Rgb(0xD9, 0x77, 0x57);
const UNFOCUSED: Color = Color::Rgb(0x66, 0x66, 0x66);

pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState, focused: bool) {
    state.preview_area_height = area.height;

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

    let mut text = ansi_to_tui::IntoText::into_text(&state.preview_content).unwrap_or_default();
    let content_height = text.lines.len() as u16;
    state.preview_content_height = content_height;

    if state.preview_is_sticky_bottom {
        let visible_height = inner_area.height;
        state.preview_scroll_offset = content_height.saturating_sub(visible_height);
    }

    if let Some(ref sel) = state.preview_selection {
        crate::selection::apply_selection_highlight(&mut text, sel, state.preview_scroll_offset, inner_area.height);
    }

    let paragraph = Paragraph::new(text)
        .block(block)
        .scroll((state.preview_scroll_offset, 0));

    frame.render_widget(paragraph, area);

    if content_height > inner_area.height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None);
        let mut scrollbar_state =
            ScrollbarState::new(content_height.saturating_sub(inner_area.height) as usize)
                .position(state.preview_scroll_offset as usize);
        frame.render_stateful_widget(
            scrollbar,
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }
}
