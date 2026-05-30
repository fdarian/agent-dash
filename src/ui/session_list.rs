use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use crate::app::AppState;
use crate::filter_query::parse_filter_query;
use crate::notes;
use crate::session::{Agent, PromptState, SessionStatus, VisibleItem};

pub fn render(frame: &mut Frame, area: Rect, state: &AppState, focused: bool, flat_view: bool) {
    let border_color = if focused {
        state.theme.primary
    } else {
        state.theme.border_unfocused
    };
    let filter_color = state.theme.text_subtle;
    let flag_color = state.theme.accent_flag;

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
                    Style::default().fg(state.theme.text_muted),
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
                    spans.push(Span::styled(token, Style::default().fg(state.theme.text)));
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
            Line::from(" No matching sessions").fg(state.theme.border_unfocused)
        } else {
            Line::from(" No agent sessions found").fg(state.theme.border_unfocused)
        };
        frame.render_widget(text, inner);
        return;
    }

    let mut in_hidden_flags = vec![false; state.visible_items.len()];
    {
        let mut in_global = false;
        let mut in_group = false;
        for (i, item) in state.visible_items.iter().enumerate() {
            match item {
                VisibleItem::SubgroupHeader { .. } | VisibleItem::GroupHeader { .. }
                    if !in_global =>
                {
                    in_group = false;
                }
                _ => {}
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
                VisibleItem::SubgroupHeader {
                    prefix,
                    total_count,
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
                    let text = format!("{} {} {} ({})", arrow, status_icon, prefix, total_count);
                    let style = if is_selected {
                        if in_hidden_section {
                            Style::default()
                                .fg(state.theme.selected_dim_fg)
                                .bg(state.theme.bg_selected)
                        } else {
                            Style::default()
                                .fg(state.theme.selected_fg)
                                .bg(state.theme.bg_selected)
                        }
                    } else if in_hidden_section {
                        Style::default().fg(state.theme.border_unfocused)
                    } else {
                        Style::default().fg(state.theme.text)
                    };
                    ListItem::new(Line::from(text).style(style))
                }
                VisibleItem::GroupHiddenHeader {
                    count,
                    is_collapsed,
                    ..
                } => {
                    let arrow = if *is_collapsed { "▶" } else { "▼" };
                    let text = format!("  {} Hidden ({})", arrow, count);
                    let style = if is_selected {
                        Style::default()
                            .fg(state.theme.selected_dim_fg)
                            .bg(state.theme.bg_selected)
                    } else {
                        Style::default().fg(state.theme.border_unfocused)
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
                        Style::default()
                            .fg(state.theme.selected_dim_fg)
                            .bg(state.theme.bg_selected)
                    } else {
                        Style::default().fg(state.theme.border_unfocused)
                    };
                    ListItem::new(Line::from(text).style(style))
                }
                VisibleItem::GroupHeader {
                    tmux_session_name,
                    display_name,
                    session_count,
                    has_active,
                    has_unread,
                    is_collapsed,
                    in_subgroup,
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
                    let indent = if *in_subgroup { "  " } else { "" };
                    let text = format!(
                        "{}{} {} {} ({})",
                        indent, arrow, status_icon, display_name, session_count
                    );
                    let style = if is_selected {
                        if in_hidden_section {
                            Style::default()
                                .fg(state.theme.selected_dim_fg)
                                .bg(state.theme.bg_selected)
                        } else {
                            Style::default()
                                .fg(state.theme.selected_fg)
                                .bg(state.theme.bg_selected)
                        }
                    } else if in_hidden_section {
                        Style::default().fg(state.theme.border_unfocused)
                    } else {
                        Style::default().fg(state.theme.text)
                    };
                    let group_note_path = notes::group_note_path(tmux_session_name);
                    if state.notes_with_content.contains(&group_note_path) {
                        let note_style =
                            Style::default()
                                .fg(state.theme.text_subtle)
                                .bg(if is_selected {
                                    state.theme.bg_selected
                                } else {
                                    Color::Reset
                                });
                        ListItem::new(Line::from(vec![
                            Span::styled(text, style),
                            Span::styled(" 📝", note_style),
                        ]))
                    } else {
                        ListItem::new(Line::from(text).style(style))
                    }
                }
                VisibleItem::Session {
                    session,
                    display_name,
                    is_unread,
                    in_subgroup,
                    ..
                } => {
                    let (icon, default_fg) = if in_hidden_section {
                        ("○", state.theme.border_unfocused)
                    } else {
                        match (&session.status, *is_unread) {
                            (SessionStatus::Active, _) => ("●", state.theme.primary),
                            (_, true) => ("◉", state.theme.accent_warning),
                            _ => ("○", state.theme.text_dim),
                        }
                    };
                    // opencode title is static "OpenCode"; use tmux session name instead
                    let label = if session.title.is_empty()
                        || (session.agent == Agent::Opencode && session.title == "OpenCode")
                    {
                        display_name.as_str()
                    } else {
                        session.title.as_str()
                    };
                    let base_style = if is_selected {
                        Style::default()
                            .fg(state.theme.selected_fg)
                            .bg(state.theme.bg_selected)
                    } else {
                        Style::default().fg(default_fg)
                    };

                    let indent = if flat_view {
                        " "
                    } else if *in_subgroup {
                        "    "
                    } else {
                        "  "
                    };
                    let left_text = format!("{}{} {}", indent, icon, label);
                    let prompt_state = state
                        .prompt_states
                        .get(&session.pane_id)
                        .unwrap_or(&PromptState::None);
                    let inner_width = area.width.saturating_sub(2) as usize;

                    let effective_title_differs = !(session.title.is_empty()
                        || session.agent == Agent::Opencode && session.title == "OpenCode");
                    let show_group_tag =
                        !parsed.text.is_empty() && !in_hidden_section && effective_title_differs;

                    let session_note_path = notes::session_note_path(session);
                    let has_session_note = state.notes_with_content.contains(&session_note_path);
                    let note_style =
                        Style::default()
                            .fg(state.theme.text_subtle)
                            .bg(if is_selected {
                                state.theme.bg_selected
                            } else {
                                Color::Reset
                            });

                    if *prompt_state == PromptState::None || in_hidden_section {
                        if show_group_tag {
                            let tag = display_name.as_str();
                            let tag_width = tag.chars().count();
                            let note_extra = if has_session_note { 3 } else { 0 }; // " 📝" = 1 space + emoji (counts as 2 cols)
                            let left_width = inner_width.saturating_sub(tag_width + 1 + note_extra);
                            let left_padded = truncate_or_pad(&left_text, left_width);
                            let tag_style = if is_selected {
                                Style::default()
                                    .fg(state.theme.selected_dim_fg)
                                    .bg(state.theme.bg_selected)
                            } else {
                                Style::default().fg(state.theme.border_unfocused)
                            };
                            let mut spans = vec![
                                Span::styled(left_padded, base_style),
                                Span::styled(tag, tag_style),
                            ];
                            if has_session_note {
                                spans.push(Span::styled(" 📝", note_style));
                            }
                            ListItem::new(Line::from(spans))
                        } else if has_session_note {
                            ListItem::new(Line::from(vec![
                                Span::styled(left_text, base_style),
                                Span::styled(" 📝", note_style),
                            ]))
                        } else {
                            ListItem::new(Line::from(left_text).style(base_style))
                        }
                    } else {
                        let (badge_text, badge_fg) = match prompt_state {
                            PromptState::Plan => ("plan", state.theme.accent_info),
                            PromptState::Ask => ("ask", state.theme.accent_warning),
                            PromptState::None => unreachable!(),
                        };
                        let note_extra = if has_session_note { 3 } else { 0 };
                        let badge_width = badge_text.len();
                        let left_width = inner_width.saturating_sub(badge_width + 1 + note_extra);
                        let left_padded = truncate_or_pad(&left_text, left_width);

                        let mut badge_style = Style::default().fg(badge_fg);
                        if is_selected {
                            badge_style = badge_style.bg(state.theme.bg_selected);
                        }

                        let mut spans = vec![
                            Span::styled(left_padded, base_style),
                            Span::styled(badge_text, badge_style),
                        ];
                        if has_session_note {
                            spans.push(Span::styled(" 📝", note_style));
                        }
                        ListItem::new(Line::from(spans))
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
