use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use super::{forward_scroll_target, scroll_preview_down, scroll_preview_up, Action, AppState};
use crate::selection::{self, ContentPosition, PreviewSelection};

pub(super) fn handle_mouse_event(state: &mut AppState, mouse: MouseEvent) -> Option<Action> {
    if state.pending_confirm_target.is_some()
        || state.show_help
        || state.view_config.is_some()
        || state.new_session.is_some()
    {
        return None;
    }

    if state.copy_mode.is_some() && matches!(mouse.kind, MouseEventKind::Down(_)) {
        state.copy_mode = None;
        state.preview_selection = None;
    }

    let in_preview = mouse.column >= state.preview_pane_area.x
        && mouse.column < state.preview_pane_area.x + state.preview_pane_area.width
        && mouse.row >= state.preview_pane_area.y
        && mouse.row < state.preview_pane_area.y + state.preview_pane_area.height;

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if !in_preview {
                state.preview_selection = None;
                return None;
            }
            if let Some(pos) = selection::mouse_to_content_position(
                mouse.column,
                mouse.row,
                state.preview_pane_area,
                state.preview_scroll_offset,
            ) {
                state.preview_selection = Some(PreviewSelection {
                    anchor: ContentPosition {
                        row: pos.row,
                        col: pos.col,
                    },
                    cursor: ContentPosition {
                        row: pos.row,
                        col: pos.col,
                    },
                    is_dragging: true,
                });
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(ref mut sel) = state.preview_selection {
                // Clamp mouse coords to inner area bounds
                let inner_x = state.preview_pane_area.x + 1;
                let inner_y = state.preview_pane_area.y + 1;
                let inner_right = state.preview_pane_area.x + state.preview_pane_area.width - 1;
                let inner_bottom = state.preview_pane_area.y + state.preview_pane_area.height - 1;

                let clamped_col = mouse.column.clamp(inner_x, inner_right.saturating_sub(1));
                let clamped_row = mouse.row.clamp(inner_y, inner_bottom.saturating_sub(1));

                sel.cursor.col = clamped_col - inner_x;
                sel.cursor.row = (clamped_row - inner_y) + state.preview_scroll_offset;
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if let Some(ref mut sel) = state.preview_selection {
                sel.is_dragging = false;
                if sel.anchor.row == sel.cursor.row && sel.anchor.col == sel.cursor.col {
                    state.preview_selection = None;
                } else if !state.preview_content.is_empty() {
                    let text = ansi_to_tui::IntoText::into_text(&state.preview_content)
                        .unwrap_or_default();
                    let selected = selection::extract_selected_text(&text, sel);
                    if !selected.is_empty() {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(&selected);
                            state.toast_message = Some("Copied to clipboard!".to_string());
                            state.toast_deadline = Some(
                                std::time::Instant::now() + std::time::Duration::from_millis(1500),
                            );
                        }
                    }
                }
            }
        }
        MouseEventKind::ScrollDown if in_preview => {
            let col = state.preview_pane_area.width / 2;
            let row = state.preview_pane_area.height / 2;
            if let Some((target, col, row)) = forward_scroll_target(state, col, row) {
                return Some(Action::ForwardScrollDown { target, col, row });
            }
            scroll_preview_down(state);
        }
        MouseEventKind::ScrollUp if in_preview => {
            let col = state.preview_pane_area.width / 2;
            let row = state.preview_pane_area.height / 2;
            if let Some((target, col, row)) = forward_scroll_target(state, col, row) {
                return Some(Action::ForwardScrollUp { target, col, row });
            }
            scroll_preview_up(state);
        }
        _ => {}
    }
    None
}
