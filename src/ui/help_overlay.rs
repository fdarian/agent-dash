use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem};

use crate::app::AppState;
use super::keybinds::filter_keybinds;

const PRIMARY: Color = Color::Rgb(0xD9, 0x77, 0x57);

pub fn render(frame: &mut Frame, state: &AppState) {
    let area = frame.area();

    let width = area.width / 2;
    let height = area.height * 60 / 100;
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let popup_area = Rect::new(x, y, width, height);

    let bg_color = Color::Rgb(state.terminal_bg.0, state.terminal_bg.1, state.terminal_bg.2);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help - Keybinds ")
        .border_style(Style::default().fg(PRIMARY))
        .style(Style::default().bg(bg_color));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let (list_area, filter_query) = if state.help_filter_active {
        let filter_height = 1;
        let filter_area = Rect::new(inner.x, inner.y, inner.width, filter_height);
        let list_area = Rect::new(
            inner.x,
            inner.y + filter_height + 1,
            inner.width,
            inner.height.saturating_sub(filter_height + 1),
        );

        let filter_text = if state.help_filter_query.is_empty() {
            Line::from("Type to filter...").fg(Color::Rgb(0x66, 0x66, 0x66))
        } else {
            Line::from(format!("/{}", state.help_filter_query)).fg(Color::White)
        };
        frame.render_widget(filter_text, filter_area);

        (list_area, state.help_filter_query.as_str())
    } else {
        (inner, "")
    };

    let entries = filter_keybinds(filter_query);

    if entries.is_empty() {
        let text = Line::from("No matching keybinds").fg(Color::Rgb(0x66, 0x66, 0x66));
        frame.render_widget(text, list_area);
        return;
    }

    let items: Vec<ListItem> = entries
        .iter()
        .map(|entry| {
            let key_padded = format!("{:<8}", entry.key);
            ListItem::new(
                Line::from(format!("{} {}", key_padded, entry.description))
                    .fg(Color::Rgb(0xCC, 0xCC, 0xCC)),
            )
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, list_area);
}
