use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState};

use crate::app::new_session::{Field, NewSessionDialog};
use crate::app::AppState;

const LABEL_WIDTH: u16 = 7;

pub fn render(frame: &mut Frame, state: &AppState, dialog: &NewSessionDialog) {
    let area = frame.area();
    let width = (area.width * 7 / 10).max(50).min(area.width);
    let height = (area.height * 7 / 10).max(14).min(area.height);
    let popup_area = Rect::new(
        area.width.saturating_sub(width) / 2,
        area.height.saturating_sub(height) / 2,
        width,
        height,
    );

    frame.render_widget(Clear, popup_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" New session ")
        .border_style(Style::default().fg(state.theme.primary));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // The rows depend on the config, never on the dialog's state, so nothing moves while typing.
    let mut constraints = vec![Constraint::Length(1)];
    if dialog.modes_available() {
        constraints.push(Constraint::Length(1));
    }
    if dialog.worktree_available() {
        constraints.extend([Constraint::Length(1); 4]);
    }
    constraints.extend([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ]);
    let rows = Layout::vertical(constraints).split(inner);

    render_input(frame, state, dialog, rows[0], Field::Folder);
    let mut next_row = 1;

    if dialog.modes_available() {
        render_mode(frame, state, dialog, rows[next_row]);
        next_row += 1;
    }

    if dialog.worktree_available() {
        let mark = if dialog.worktree { "x" } else { " " };
        frame.render_widget(
            Line::from(format!("[{mark}] Worktree"))
                .style(label_style(state, dialog.focus == Field::Worktree)),
            rows[next_row],
        );
        render_input(frame, state, dialog, rows[next_row + 1], Field::Name);
        render_input(frame, state, dialog, rows[next_row + 2], Field::Base);
        if let Some(status) = dialog.status_line() {
            frame.render_widget(
                Line::from(format!("{}{status}", " ".repeat(LABEL_WIDTH as usize)))
                    .fg(state.theme.text_subtle),
                rows[next_row + 3],
            );
        }
    }

    let message_row = rows.len() - 2;
    let message = match (&dialog.error, &dialog.branch_error) {
        (Some(error), _) => Line::from(error.as_str()).fg(state.theme.accent_warning),
        (None, Some(reason)) if dialog.worktree => {
            Line::from(reason.as_str()).fg(state.theme.text_subtle)
        }
        _ => Line::default(),
    };
    frame.render_widget(message, rows[message_row]);

    render_suggestions(frame, state, dialog, rows[message_row + 1]);
}

fn label_style(state: &AppState, focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(state.theme.primary)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(state.theme.text_subtle)
    }
}

fn label(field: Field) -> &'static str {
    match field {
        Field::Folder => "Folder",
        Field::Mode => "Mode",
        Field::Worktree => "Worktree",
        Field::Name => "Name",
        Field::Base => "Base",
    }
}

fn render_input(
    frame: &mut Frame,
    state: &AppState,
    dialog: &NewSessionDialog,
    area: Rect,
    field: Field,
) {
    let (value, cursor, dimmed) = match field {
        Field::Name => (&dialog.name, dialog.name_cursor, dialog.name_dimmed()),
        Field::Base => (&dialog.base, dialog.base_cursor, dialog.base_dimmed()),
        _ => (&dialog.folder, dialog.folder_cursor, false),
    };
    let focused = dialog.focus == field;
    let [label_area, input_area] =
        Layout::horizontal([Constraint::Length(LABEL_WIDTH), Constraint::Min(1)]).areas(area);
    let label_style = if dimmed {
        Style::default().fg(state.theme.text_muted)
    } else {
        label_style(state, focused)
    };
    frame.render_widget(Line::from(label(field)).style(label_style), label_area);
    let placeholder = match field {
        Field::Folder => Some(
            dialog
                .folder_placeholder()
                .unwrap_or("type a path or pick one below"),
        ),
        Field::Name => Some("random if blank"),
        Field::Mode | Field::Worktree | Field::Base => None,
    };
    let input = if let (true, Some(placeholder)) = (value.is_empty(), placeholder) {
        Line::from(placeholder).fg(state.theme.text_muted)
    } else {
        let color = if dimmed {
            state.theme.text_muted
        } else {
            state.theme.text
        };
        Line::from(value.as_str()).fg(color)
    };
    frame.render_widget(input, input_area);
    if focused {
        let x = input_area.x + (cursor as u16).min(input_area.width.saturating_sub(1));
        frame.set_cursor_position((x, input_area.y));
    }
}

fn render_mode(frame: &mut Frame, state: &AppState, dialog: &NewSessionDialog, area: Rect) {
    let [label_area, value_area] =
        Layout::horizontal([Constraint::Length(LABEL_WIDTH), Constraint::Min(1)]).areas(area);
    frame.render_widget(
        Line::from(label(Field::Mode)).style(label_style(state, dialog.focus == Field::Mode)),
        label_area,
    );
    let mut spans = Vec::new();
    for (index, mode) in dialog.modes().iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        let style = if index == dialog.mode {
            Style::default()
                .fg(state.theme.selected_fg)
                .bg(state.theme.bg_selected)
        } else {
            Style::default().fg(state.theme.text)
        };
        spans.push(Span::styled(format!("{index} {}", mode.name), style));
    }
    frame.render_widget(Line::from(spans), value_area);
}

fn render_mode_rows(frame: &mut Frame, state: &AppState, dialog: &NewSessionDialog, area: Rect) {
    let name_width = dialog
        .modes()
        .iter()
        .map(|mode| mode.name.chars().count())
        .max()
        .unwrap_or(0);
    let items: Vec<ListItem> = dialog
        .modes()
        .iter()
        .enumerate()
        .map(|(index, mode)| {
            let selected = index == dialog.mode;
            let (name_style, command_style) = if selected {
                let style = Style::default()
                    .fg(state.theme.selected_fg)
                    .bg(state.theme.bg_selected);
                (style, style)
            } else {
                (
                    Style::default().fg(state.theme.text),
                    Style::default().fg(state.theme.text_subtle),
                )
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {index}  {:<name_width$}  ", mode.name),
                    name_style,
                ),
                Span::styled(mode.command.as_str(), command_style),
            ]))
        })
        .collect();
    let mut list_state = ListState::default().with_selected(Some(dialog.mode));
    frame.render_stateful_widget(List::new(items), area, &mut list_state);
}

fn render_suggestions(frame: &mut Frame, state: &AppState, dialog: &NewSessionDialog, area: Rect) {
    let [header_area, list_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    let (header, empty) = match dialog.focus {
        Field::Folder => ("Folders", "No matching folders"),
        Field::Mode => ("Modes", ""),
        Field::Name | Field::Base => ("Branches", "No matching branches"),
        Field::Worktree => ("", "Space toggles the worktree option"),
    };
    frame.render_widget(
        Line::from(format!(" {header}")).fg(state.theme.text_subtle),
        header_area,
    );
    if dialog.focus == Field::Mode {
        render_mode_rows(frame, state, dialog, list_area);
        return;
    }
    let suggestions = dialog.suggestions();
    if suggestions.is_empty() {
        frame.render_widget(
            Line::from(format!(" {empty}")).fg(state.theme.text_muted),
            list_area,
        );
        return;
    }
    let highlighted = dialog.highlight_index();
    let items: Vec<ListItem> = suggestions
        .iter()
        .enumerate()
        .map(|(i, suggestion)| {
            let style = if Some(i) == highlighted {
                Style::default()
                    .fg(state.theme.selected_fg)
                    .bg(state.theme.bg_selected)
            } else {
                Style::default().fg(state.theme.text)
            };
            ListItem::new(Line::from(format!(" {suggestion}")).style(style))
        })
        .collect();
    let mut list_state = ListState::default().with_selected(highlighted);
    frame.render_stateful_widget(List::new(items), list_area, &mut list_state);
}
