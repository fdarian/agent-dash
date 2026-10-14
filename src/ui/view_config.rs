use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState};

use crate::app::view_config::{Row, ViewConfigOverlay};
use crate::app::AppState;

pub fn render(frame: &mut Frame, state: &AppState, overlay: &ViewConfigOverlay) {
    let area = frame.area();
    let (title, lines, cursor) = match *overlay {
        ViewConfigOverlay::Rows { cursor } => (
            " View ".to_string(),
            Row::ALL
                .iter()
                .map(|row| {
                    format!(
                        "{} {}: {}",
                        row.index() + 1,
                        row.label(),
                        row.value_label(&state.ui.view)
                    )
                })
                .collect::<Vec<_>>(),
            cursor.index(),
        ),
        ViewConfigOverlay::Selector { row, cursor } => {
            let current = row.current_index(&state.ui.view);
            (
                format!(" {} ", row.label()),
                row.options()
                    .iter()
                    .enumerate()
                    .map(|(i, label)| {
                        let mark = if i == current { "*" } else { " " };
                        format!("{mark} {label}")
                    })
                    .collect(),
                cursor,
            )
        }
    };

    let width = 36.min(area.width);
    let height = (lines.len() as u16 + 2).min(area.height);
    let popup_area = Rect::new(
        area.width.saturating_sub(width) / 2,
        area.height.saturating_sub(height) / 2,
        width,
        height,
    );

    frame.render_widget(Clear, popup_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(state.theme.primary));

    let items: Vec<ListItem> = lines
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let style = if i == cursor {
                Style::default()
                    .fg(state.theme.selected_fg)
                    .bg(state.theme.bg_selected)
            } else {
                Style::default().fg(state.theme.text)
            };
            ListItem::new(Line::from(format!(" {text}")).style(style))
        })
        .collect();
    let list = List::new(items).block(block);
    let mut list_state = ListState::default().with_selected(Some(cursor));
    frame.render_stateful_widget(list, popup_area, &mut list_state);
}
