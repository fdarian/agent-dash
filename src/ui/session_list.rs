use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use crate::app::AppState;
use crate::filter_query::parse_filter_query;
use crate::session::{PromptState, SessionStatus, VisibleItem};

const PRIMARY: Color = Color::Rgb(0xD9, 0x77, 0x57);
const UNFOCUSED: Color = Color::Rgb(0x66, 0x66, 0x66);
const UNREAD: Color = Color::Rgb(0xE5, 0xC0, 0x7B);
const IDLE: Color = Color::Rgb(0xAA, 0xAA, 0xAA);
const SELECTED_BG: Color = Color::Rgb(0x44, 0x44, 0x44);

pub fn render(frame: &mut Frame, area: Rect, state: &AppState, focused: bool, flat_view: bool) {
    let border_color = if focused { PRIMARY } else { UNFOCUSED };
    let filter_color = Color::Rgb(0x88, 0x88, 0x88);
    let flag_color = Color::Rgb(0x61, 0x96, 0xCC);

    let parsed = parse_filter_query(&state.session_filter_query);

    let mut block = Block::default()
        .borders(Borders::ALL)
        .title(" [1] Sessions ")
        .border_style(Style::default().fg(border_color));

    if state.session_filter_active || !state.session_filter_query.is_empty() {
        let filter_line = if state.session_filter_query.is_empty() {
            Line::from(vec![
                Span::styled("/", Style::default().fg(filter_color)),
                Span::styled(
                    "Type to filter...",
                    Style::default().fg(Color::Rgb(0x55, 0x55, 0x55)),
                ),
                Span::raw(" "),
            ])
        } else {
            let mut spans = vec![Span::styled("/", Style::default().fg(filter_color))];
            for (i, token) in state.session_filter_query.split_whitespace().enumerate() {
                if i > 0 {
                    spans.push(Span::raw(" "));
                }
                let lower = token.to_lowercase();
                if lower == "is:h" || lower == "is:hidden" {
                    spans.push(Span::styled(token, Style::default().fg(flag_color)));
                } else {
                    spans.push(Span::styled(token, Style::default().fg(Color::White)));
                }
            }
            spans.push(Span::raw(" "));
            Line::from(spans)
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
        let text = if !state.session_filter_query.is_empty() {
            Line::from(" No matching sessions").fg(UNFOCUSED)
        } else {
            Line::from(" No agent sessions found").fg(UNFOCUSED)
        };
        frame.render_widget(text, inner);
        return;
    }

    let mut in_hidden_flags = vec![false; state.visible_items.len()];
    {
        let mut in_global = false;
        let mut in_group = false;
        for (i, item) in state.visible_items.iter().enumerate() {
            if matches!(item, VisibleItem::GroupHeader { .. }) && !in_global {
                in_group = false;
            }
            if in_global || in_group {
                in_hidden_flags[i] = true;
            }
            match item {
                VisibleItem::HiddenHeader { .. } => {
                    in_global = true;
                    in_group = false;
                }
                VisibleItem::GroupHiddenHeader { .. } => {
                    in_group = true;
                }
                _ => {}
            }
        }
    }

    let items: Vec<ListItem> = state
        .visible_items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = i == state.selected_index;
            let in_hidden_section = in_hidden_flags[i];
            match item {
                VisibleItem::GroupHiddenHeader {
                    count,
                    is_collapsed,
                    ..
                } => {
                    let arrow = if *is_collapsed { "▶" } else { "▼" };
                    let text = format!("  {} Hidden ({})", arrow, count);
                    let style = if is_selected {
                        Style::default().fg(UNFOCUSED).bg(SELECTED_BG)
                    } else {
                        Style::default().fg(UNFOCUSED)
                    };
                    ListItem::new(Line::from(text).style(style))
                }
                VisibleItem::HiddenHeader {
                    count,
                    is_collapsed,
                } => {
                    let arrow = if *is_collapsed { "▶" } else { "▼" };
                    let text = format!("{} Hidden ({})", arrow, count);
                    let style = if is_selected {
                        Style::default().fg(UNFOCUSED).bg(SELECTED_BG)
                    } else {
                        Style::default().fg(UNFOCUSED)
                    };
                    ListItem::new(Line::from(text).style(style))
                }
                VisibleItem::GroupHeader {
                    display_name,
                    session_count,
                    has_active,
                    has_unread,
                    is_collapsed,
                    ..
                } => {
                    let arrow = if *is_collapsed { "▶" } else { "▼" };
                    let status_icon = if *has_active {
                        "●"
                    } else if *has_unread {
                        "◉"
                    } else {
                        "○"
                    };
                    let text = format!(
                        "{} {} {} ({})",
                        arrow, status_icon, display_name, session_count
                    );
                    let style = if is_selected {
                        if in_hidden_section {
                            Style::default().fg(UNFOCUSED).bg(SELECTED_BG)
                        } else {
                            Style::default().fg(Color::White).bg(SELECTED_BG)
                        }
                    } else if in_hidden_section {
                        Style::default().fg(UNFOCUSED)
                    } else {
                        Style::default().fg(Color::Rgb(0xCC, 0xCC, 0xCC))
                    };
                    ListItem::new(Line::from(text).style(style))
                }
                VisibleItem::Session {
                    session,
                    display_name,
                    is_unread,
                    ..
                } => {
                    let (icon, default_fg) = if in_hidden_section {
                        ("○", UNFOCUSED)
                    } else {
                        match (&session.status, *is_unread) {
                            (SessionStatus::Active, _) => ("●", PRIMARY),
                            (_, true) => ("◉", UNREAD),
                            _ => ("○", IDLE),
                        }
                    };
                    let label = if session.title.is_empty() {
                        display_name.as_str()
                    } else {
                        session.title.as_str()
                    };
                    let base_style = if is_selected {
                        Style::default().fg(Color::White).bg(SELECTED_BG)
                    } else {
                        Style::default().fg(default_fg)
                    };

                    let indent = if flat_view { " " } else { "  " };
                    let left_text = format!("{}{} {}", indent, icon, label);
                    let prompt_state = state
                        .prompt_states
                        .get(&session.pane_id)
                        .unwrap_or(&PromptState::None);
                    let inner_width = area.width.saturating_sub(2) as usize;

                    let show_group_tag =
                        !parsed.text.is_empty() && !in_hidden_section && !session.title.is_empty();

                    if *prompt_state == PromptState::None || in_hidden_section {
                        if show_group_tag {
                            let tag = display_name.as_str();
                            let tag_width = tag.chars().count();
                            let left_width = inner_width.saturating_sub(tag_width + 1);
                            let left_padded = truncate_or_pad(&left_text, left_width);
                            let tag_style = if is_selected {
                                Style::default().fg(UNFOCUSED).bg(SELECTED_BG)
                            } else {
                                Style::default().fg(UNFOCUSED)
                            };
                            ListItem::new(Line::from(vec![
                                Span::styled(left_padded, base_style),
                                Span::styled(tag, tag_style),
                            ]))
                        } else {
                            ListItem::new(Line::from(left_text).style(base_style))
                        }
                    } else {
                        let (badge_text, badge_fg) = match prompt_state {
                            PromptState::Plan => ("plan", Color::Rgb(0x61, 0xAF, 0xEF)),
                            PromptState::Ask => ("ask", Color::Rgb(0xE5, 0xC0, 0x7B)),
                            PromptState::None => unreachable!(),
                        };
                        let badge_width = badge_text.len();
                        let left_width = inner_width.saturating_sub(badge_width + 1);
                        let left_padded = truncate_or_pad(&left_text, left_width);

                        let mut badge_style = Style::default().fg(badge_fg);
                        if is_selected {
                            badge_style = badge_style.bg(SELECTED_BG);
                        }

                        ListItem::new(Line::from(vec![
                            Span::styled(left_padded, base_style),
                            Span::styled(badge_text, badge_style),
                        ]))
                    }
                }
            }
        })
        .collect();

    let list = List::new(items).block(block);
    let mut list_state = ListState::default().with_selected(Some(state.selected_index));
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn truncate_or_pad(text: &str, width: usize) -> String {
    let char_count = text.chars().count();
    if char_count > width {
        let truncated: String = text.chars().take(width.saturating_sub(1)).collect();
        format!("{}~", truncated)
    } else {
        format!("{:width$}", text, width = width)
    }
}
