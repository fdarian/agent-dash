use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::app::AppState;
use crate::session::{PromptState, SessionStatus, VisibleItem};

const PRIMARY: Color = Color::Rgb(0xD9, 0x77, 0x57);
const UNFOCUSED: Color = Color::Rgb(0x66, 0x66, 0x66);
const UNREAD: Color = Color::Rgb(0xE5, 0xC0, 0x7B);
const IDLE: Color = Color::Rgb(0xAA, 0xAA, 0xAA);
const SELECTED_BG: Color = Color::Rgb(0x44, 0x44, 0x44);

pub fn render(frame: &mut Frame, area: Rect, state: &AppState, focused: bool) {
    let border_color = if focused { PRIMARY } else { UNFOCUSED };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" [1] Sessions ")
        .border_style(Style::default().fg(border_color));

    if state.visible_items.is_empty() {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let text = Line::from(" No Claude sessions found").fg(UNFOCUSED);
        frame.render_widget(text, inner);
        return;
    }

    let items: Vec<ListItem> = state
        .visible_items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = i == state.selected_index;
            match item {
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
                        Style::default().fg(Color::White).bg(SELECTED_BG)
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
                    let (icon, default_fg) = match (&session.status, *is_unread) {
                        (SessionStatus::Active, _) => ("●", PRIMARY),
                        (_, true) => ("◉", UNREAD),
                        _ => ("○", IDLE),
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

                    let left_text = format!("  {} {}", icon, label);
                    let prompt_state = state
                        .prompt_states
                        .get(&session.pane_id)
                        .unwrap_or(&PromptState::None);
                    let inner_width = area.width.saturating_sub(2) as usize;

                    if *prompt_state == PromptState::None {
                        ListItem::new(Line::from(left_text).style(base_style))
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
    frame.render_widget(list, area);
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
