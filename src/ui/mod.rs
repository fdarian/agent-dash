use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::{AppState, Focus};
use crate::config::LayoutDirection;
use crate::notes;
use crate::session::VisibleItem;

pub mod bottom_bar;
pub mod confirm_dialog;
pub mod help_overlay;
pub mod keybinds;
pub mod pane_note;
pub mod pane_preview;
pub mod session_list;
pub mod theme;

pub fn render(frame: &mut Frame, state: &mut AppState) {
    let [main_area, bar_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());

    match state.focus {
        Focus::Sessions => {
            if state.sessions_expanded {
                session_list::render(frame, main_area, state, true, state.flat_view);
                state.preview_pane_area = Rect::default();
            } else {
                let chunks = match state.config.layout {
                    LayoutDirection::Vertical => {
                        Layout::vertical([Constraint::Percentage(30), Constraint::Min(1)])
                            .split(main_area)
                    }
                    LayoutDirection::Horizontal => {
                        Layout::horizontal([Constraint::Length(40), Constraint::Min(1)])
                            .split(main_area)
                    }
                };
                session_list::render(frame, chunks[0], state, true, state.flat_view);

                let note = note_for_selected(state);
                let selected_is_group = matches!(
                    state.visible_items.get(state.selected_index),
                    Some(VisibleItem::GroupHeader { .. })
                );
                match note {
                    // Groups have no single pane to preview, so the note fills the column.
                    Some(content) if selected_is_group => {
                        state.preview_pane_area = Rect::default();
                        pane_note::render(frame, chunks[1], state, &content);
                    }
                    // A session with a note: note on top, live pane preview below.
                    Some(content) => {
                        let note_height = note_pane_height(&content, chunks[1].height);
                        let [note_area, preview_area] =
                            Layout::vertical([Constraint::Length(note_height), Constraint::Min(1)])
                                .areas(chunks[1]);
                        pane_note::render(frame, note_area, state, &content);
                        state.preview_pane_area = preview_area;
                        pane_preview::render(frame, preview_area, state, false);
                    }
                    None => {
                        state.preview_pane_area = chunks[1];
                        pane_preview::render(frame, chunks[1], state, false);
                    }
                }
            }
        }
        Focus::Preview => {
            state.preview_pane_area = main_area;
            pane_preview::render(frame, main_area, state, true);
        }
    }

    bottom_bar::render(frame, bar_area, state);

    // Overlays rendered on top of main layout
    if state.pending_confirm_target.is_some() {
        confirm_dialog::render(frame, state);
    }
    if state.show_help {
        help_overlay::render(frame, state);
    }

    if let Some(ref msg) = state.toast_message {
        let area = frame.area();
        let toast_width = (msg.len() + 2) as u16;
        let toast_area = Rect::new(
            area.width.saturating_sub(toast_width + 1),
            1,
            toast_width,
            1,
        );
        let toast = Paragraph::new(format!(" {} ", msg))
            .style(Style::default().fg(Color::Black).bg(state.theme.primary));
        frame.render_widget(toast, toast_area);
    }
}

/// Reads the note content for the currently selected session or group, returning
/// `None` when there is no selection, the item type has no note, or the note is empty.
fn note_for_selected(state: &AppState) -> Option<String> {
    let item = state.visible_items.get(state.selected_index)?;
    let path = match item {
        VisibleItem::Session { session, .. } => notes::session_note_path(session),
        VisibleItem::GroupHeader {
            tmux_session_name, ..
        } => notes::group_note_path(tmux_session_name),
        _ => return None,
    };
    notes::read_note(&path)
}

/// Sizes the note pane to its content (plus borders), clamped between 3 rows and
/// half the available height so a long note never crowds out the pane preview.
fn note_pane_height(content: &str, available: u16) -> u16 {
    let desired = content.lines().count() as u16 + 2;
    let max = (available / 2).max(3);
    desired.clamp(3, max)
}
