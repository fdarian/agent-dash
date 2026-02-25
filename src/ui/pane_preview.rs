use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};

use crate::app::AppState;

const PRIMARY: Color = Color::Rgb(0xD9, 0x77, 0x57);
const UNFOCUSED: Color = Color::Rgb(0x66, 0x66, 0x66);

pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState, focused: bool) {
    state.preview_area_height = area.height;

    let border_color = if focused { PRIMARY } else { UNFOCUSED };

    let title = if state.copy_mode.is_some() {
        " [0] Preview [COPY] "
    } else {
        " [0] Preview "
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
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

    if let Some(ref copy) = state.copy_mode {
        if !copy.search_matches.is_empty() {
            crate::copy_mode::apply_search_highlights(
                &mut text,
                &copy.search_matches,
                copy.current_match_index,
                state.preview_scroll_offset,
                inner_area.height,
            );
        }
        crate::copy_mode::apply_cursor_highlight(
            &mut text,
            copy.cursor.row,
            copy.cursor.col,
            state.preview_scroll_offset,
            inner_area.height,
        );
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

    if let Some(ref copy) = state.copy_mode {
        if copy.search_active {
            let search_y = inner_area.y + inner_area.height.saturating_sub(1);
            let search_area = Rect::new(inner_area.x, search_y, inner_area.width, 1);
            let bg = Color::Rgb(0x33, 0x33, 0x33);
            let spans = vec![
                Span::styled("/", Style::default().fg(Color::Rgb(0x88, 0x88, 0x88)).bg(bg)),
                Span::styled(copy.search_query.as_str(), Style::default().fg(Color::White).bg(bg)),
            ];
            let search_paragraph = Paragraph::new(Line::from(spans))
                .style(Style::default().bg(bg));
            frame.render_widget(search_paragraph, search_area);
            let cursor_x = search_area.x + 1 + copy.search_cursor as u16;
            frame.set_cursor_position((cursor_x, search_area.y));
        }
    }
}
