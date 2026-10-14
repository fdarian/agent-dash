use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

use crate::app::AppState;
use crate::sessions::Session;
use crate::time_fmt::format_relative_time;

const MAX_PROMPT_CHARS: usize = 600;

pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState, focused: bool) {
    state.preview_area_height = area.height;

    let border_color = if focused {
        state.theme.primary
    } else {
        state.theme.border_unfocused
    };

    let session_id_suffix = state
        .visible_items
        .get(state.selected_index)
        .and_then(|item| item.session())
        .map(|session| {
            let short_id: String = session.record.session_id.chars().take(8).collect();
            format!(" · {} ", short_id)
        })
        .unwrap_or_default();

    let title_str;
    let title = if session_id_suffix.is_empty() {
        if state.copy_mode.is_some() {
            " [0] Preview [COPY] "
        } else {
            " [0] Preview "
        }
    } else {
        title_str = if state.copy_mode.is_some() {
            format!(" [0] Preview [COPY]{}", session_id_suffix)
        } else {
            format!(" [0] Preview{}", session_id_suffix)
        };
        title_str.as_str()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(border_color));

    let inner_area = block.inner(area);

    if let Some(session) = state
        .visible_items
        .get(state.selected_index)
        .and_then(|item| item.session())
        .filter(|session| !session.is_live())
    {
        render_closed_placeholder(frame, area, block, state, session);
        return;
    }

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

    if let Some(ref copy) = state.copy_mode {
        if !copy.search_matches.is_empty() {
            crate::copy_mode::apply_search_highlights(
                &mut text,
                &copy.search_matches,
                copy.current_match_index,
                state.preview_scroll_offset,
                inner_area.height,
                state.theme,
            );
        }
    }

    if let Some(ref sel) = state.preview_selection {
        crate::selection::apply_selection_highlight(
            &mut text,
            sel,
            state.preview_scroll_offset,
            inner_area.height,
            state.theme,
        );
    }

    if let Some(ref copy) = state.copy_mode {
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
            let bg = state.theme.bg_subtle;
            let prefix = if copy.search_direction == crate::copy_mode::SearchDirection::Backward {
                "?"
            } else {
                "/"
            };
            let spans = vec![
                Span::styled(prefix, Style::default().fg(state.theme.text_subtle).bg(bg)),
                Span::styled(
                    copy.search_query.as_str(),
                    Style::default().fg(state.theme.text).bg(bg),
                ),
            ];
            let search_paragraph = Paragraph::new(Line::from(spans)).style(Style::default().bg(bg));
            frame.render_widget(ratatui::widgets::Clear, search_area);
            frame.render_widget(search_paragraph, search_area);
            let cursor_x = search_area.x + 1 + copy.search_cursor as u16;
            frame.set_cursor_position((cursor_x, search_area.y));
        }
    }
}

fn render_closed_placeholder(
    frame: &mut Frame,
    area: Rect,
    block: Block,
    state: &AppState,
    session: &Session,
) {
    let theme = state.theme;
    let label = Style::default().fg(theme.text_subtle);
    let value = Style::default().fg(theme.text);

    let last_activity = session.record.last_activity as i64;
    let absolute = chrono::DateTime::from_timestamp(last_activity, 0).map(|utc| {
        utc.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    });
    let ago = format_relative_time(chrono::Local::now().timestamp(), last_activity);
    let last_activity_text = match absolute {
        Some(absolute) => format!("{ago} ago ({absolute})"),
        None => format!("{ago} ago"),
    };

    let mut lines = vec![
        Line::styled(
            session.display_title(),
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
        Line::from(vec![
            Span::styled("cwd            ", label),
            Span::styled(session.record.cwd.clone(), value),
        ]),
        Line::from(vec![
            Span::styled("last activity  ", label),
            Span::styled(last_activity_text, value),
        ]),
        Line::default(),
    ];
    match state.closed_session_prompt.as_deref() {
        Some(prompt) => {
            lines.push(Line::styled("last prompt", label));
            lines.extend(
                truncate_chars(prompt, MAX_PROMPT_CHARS)
                    .lines()
                    .map(|line| Line::styled(line.to_string(), value)),
            );
        }
        None => lines.push(Line::styled("no prompt recorded", label)),
    }
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("press ", label),
        Span::styled(
            "o",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" to resume", label),
    ]));

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(ratatui::widgets::Wrap { trim: false }),
        area,
    );
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max).collect();
    format!("{kept}…")
}
