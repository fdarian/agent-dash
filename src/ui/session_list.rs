use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::AppState;
use crate::sessions::live::PromptState;
use crate::sessions::view::{GroupHeader, Grouping, VisibleItem};
use crate::sessions::Session;
use crate::time_fmt::format_relative_time;

/// Wrapped rather than truncated: the list can be as narrow as 40 columns.
const EMPTY_STORE_HINT: &str =
    "No sessions yet. Run `agent-dash import` to bring in existing Claude sessions.";

pub fn render(frame: &mut Frame, area: Rect, state: &AppState, focused: bool) {
    let border_color = if focused {
        state.theme.primary
    } else {
        state.theme.border_unfocused
    };
    let filter_color = state.theme.text_subtle;
    let now_epoch = chrono::Local::now().timestamp();

    let grouping_label = match state.ui.view.grouping {
        Grouping::State => "state",
        Grouping::Folder => "folder",
        Grouping::Date => "date",
    };
    let mut block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" [1] Sessions · by {} ", grouping_label))
        .border_style(Style::default().fg(border_color));

    if state.session_filter_active || !state.session_filter_query.is_empty() {
        let filter_line = if state.session_filter_query.is_empty() {
            Line::from(vec![
                Span::styled("/", Style::default().fg(filter_color)),
                Span::styled(
                    "Type to filter...",
                    Style::default().fg(state.theme.text_muted),
                ),
                Span::raw(" "),
            ])
        } else {
            Line::from(vec![
                Span::styled("/", Style::default().fg(filter_color)),
                Span::styled(
                    state.session_filter_query.as_str(),
                    Style::default().fg(state.theme.text),
                ),
                Span::raw(" "),
            ])
        };
        block = block.title_bottom(filter_line);

        if state.session_filter_active {
            let cursor_x = area.x + 1 + 1 + state.session_filter_cursor as u16;
            frame.set_cursor_position((cursor_x, area.y + area.height - 1));
        }
    }

    if state.visible_items.is_empty() {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if !state.session_filter_query.is_empty() {
            frame.render_widget(
                Line::from(" No matching sessions").fg(state.theme.text_subtle),
                inner,
            );
        } else if state.book.is_empty() {
            frame.render_widget(
                Paragraph::new(EMPTY_STORE_HINT)
                    .style(Style::default().fg(state.theme.text_subtle))
                    .wrap(Wrap { trim: true }),
                inner.inner(Margin::new(1, 0)),
            );
        } else {
            frame.render_widget(
                Line::from(" No agent sessions found").fg(state.theme.text_subtle),
                inner,
            );
        }
        return;
    }

    let searching = !state.session_filter_query.trim().is_empty();
    let inner_width = area.width.saturating_sub(2) as usize;
    let items: Vec<ListItem> = state
        .visible_items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = i == state.selected_index;
            match item {
                VisibleItem::Group(group) => group_row(state, group, is_selected),
                VisibleItem::Session(session) => session_row(
                    state,
                    session,
                    is_selected,
                    searching,
                    inner_width,
                    now_epoch,
                ),
            }
        })
        .collect();

    let list = List::new(items).block(block);
    let mut list_state = ListState::default().with_selected(Some(state.selected_index));
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn group_row<'a>(state: &AppState, group: &GroupHeader, is_selected: bool) -> ListItem<'a> {
    let arrow = if group.is_collapsed { "▶" } else { "▼" };
    let status_icon = if group.has_busy {
        "●"
    } else if group.has_unread {
        "◉"
    } else {
        "○"
    };
    let text = format!(
        "{} {} {} ({})",
        arrow, status_icon, group.label, group.count
    );
    let style = if is_selected {
        Style::default()
            .fg(state.theme.selected_fg)
            .bg(state.theme.bg_selected)
    } else {
        Style::default().fg(state.theme.text)
    };
    ListItem::new(Line::from(text).style(style))
}

fn session_row<'a>(
    state: &AppState,
    session: &Session,
    is_selected: bool,
    searching: bool,
    inner_width: usize,
    now_epoch: i64,
) -> ListItem<'a> {
    let (icon, icon_color) = if !session.is_live() {
        (
            if session.record.unread { "◉" } else { "○" },
            state.theme.text_muted,
        )
    } else if session.is_busy() {
        ("●", state.theme.primary)
    } else if session.record.unread {
        ("◉", state.theme.accent_warning)
    } else {
        ("○", state.theme.text_dim)
    };
    let (title_style, icon_style, muted_style) = if is_selected {
        let on_selected = Style::default().bg(state.theme.bg_selected);
        (
            on_selected.fg(state.theme.selected_fg),
            on_selected.fg(state.theme.selected_fg),
            on_selected.fg(state.theme.selected_dim_fg),
        )
    } else {
        (
            Style::default().fg(state.theme.text),
            Style::default().fg(icon_color),
            Style::default().fg(state.theme.text_muted),
        )
    };

    let indent = if searching { " " } else { "  " };
    let prefix = format!("{indent}{icon} ");

    let mut right: Vec<Span> = Vec::new();
    match session.prompt_state() {
        PromptState::Plan => right.push(badge("plan", state.theme.accent_info, is_selected, state)),
        PromptState::Ask => {
            right.push(badge("ask", state.theme.accent_warning, is_selected, state))
        }
        PromptState::None => {}
    }
    if session.record.archived_at.is_some() {
        right.push(Span::styled("archived", muted_style));
    }
    if state.ui.view.grouping != Grouping::Folder {
        right.push(Span::styled(session.folder_name(), muted_style));
    }
    right.push(Span::styled(
        format_relative_time(now_epoch, session.record.last_activity as i64),
        muted_style,
    ));

    // One space between right-hand items, and one before the first.
    let right_width: usize = right
        .iter()
        .map(|span| span.content.chars().count() + 1)
        .sum();
    let title_width = inner_width
        .saturating_sub(right_width)
        .saturating_sub(prefix.chars().count());
    let mut spans = vec![
        Span::styled(prefix, icon_style),
        Span::styled(
            truncate_or_pad(&session.display_title(), title_width),
            title_style,
        ),
    ];
    for span in right {
        spans.push(Span::styled(" ", title_style));
        spans.push(span);
    }
    ListItem::new(Line::from(spans))
}

fn badge<'a>(text: &'static str, fg: Color, is_selected: bool, state: &AppState) -> Span<'a> {
    let mut style = Style::default().fg(fg);
    if is_selected {
        style = style.bg(state.theme.bg_selected);
    }
    Span::styled(text, style)
}

pub(super) fn truncate_or_pad(text: &str, width: usize) -> String {
    let char_count = text.chars().count();
    if char_count > width {
        let truncated: String = text.chars().take(width.saturating_sub(1)).collect();
        format!("{}~", truncated)
    } else {
        format!("{:width$}", text, width = width)
    }
}
