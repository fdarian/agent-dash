use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::prelude::*;

use crate::app::{Action, AppState};
use crate::selection::{self, ContentPosition, PreviewSelection};

#[derive(Clone, Copy, PartialEq)]
pub enum SearchDirection {
    Forward,
    Backward,
}

pub struct SearchMatch {
    pub row: u16,
    pub col: u16,
    pub len: u16,
}

pub struct CopyModeState {
    pub cursor: ContentPosition,
    pub anchor: Option<ContentPosition>,
    pub pending_g: bool,
    pub pending_z: bool,
    pub search_active: bool,
    pub search_query: String,
    pub search_cursor: usize,
    pub search_matches: Vec<SearchMatch>,
    pub current_match_index: Option<usize>,
    pub search_direction: SearchDirection,
}

impl CopyModeState {
    pub fn new(cursor_row: u16, cursor_col: u16) -> Self {
        CopyModeState {
            cursor: ContentPosition {
                row: cursor_row,
                col: cursor_col,
            },
            anchor: None,
            pending_g: false,
            pending_z: false,
            search_active: false,
            search_query: String::new(),
            search_cursor: 0,
            search_matches: Vec::new(),
            current_match_index: None,
            search_direction: SearchDirection::Forward,
        }
    }
}

fn line_char_count(text: &Text, row: u16) -> u16 {
    if (row as usize) >= text.lines.len() {
        return 0;
    }
    text.lines[row as usize]
        .spans
        .iter()
        .map(|s| s.content.chars().count() as u16)
        .sum()
}

fn clamp_col(text: &Text, row: u16, col: u16) -> u16 {
    let len = line_char_count(text, row);
    if len == 0 {
        0
    } else {
        col.min(len.saturating_sub(1))
    }
}

pub fn ensure_cursor_visible(state: &mut AppState) {
    let copy = match state.copy_mode.as_ref() {
        Some(c) => c,
        None => return,
    };
    let cursor_row = copy.cursor.row;
    let visible_height = state.preview_area_height.saturating_sub(2);
    let offset = state.preview_scroll_offset;

    if cursor_row < offset {
        state.preview_scroll_offset = cursor_row;
        state.preview_is_sticky_bottom = false;
    } else if cursor_row >= offset + visible_height {
        state.preview_scroll_offset = cursor_row.saturating_sub(visible_height.saturating_sub(1));
        state.preview_is_sticky_bottom = false;
    }
}

pub fn sync_selection(state: &mut AppState) {
    let copy = match state.copy_mode.as_ref() {
        Some(c) => c,
        None => return,
    };
    match copy.anchor.as_ref() {
        Some(anchor) => {
            state.preview_selection = Some(PreviewSelection {
                anchor: ContentPosition {
                    row: anchor.row,
                    col: anchor.col,
                },
                cursor: ContentPosition {
                    row: copy.cursor.row,
                    col: copy.cursor.col,
                },
                is_dragging: false,
            });
        }
        None => {
            state.preview_selection = None;
        }
    }
}

pub fn handle_copy_mode_key(state: &mut AppState, key: KeyEvent) -> Option<Action> {
    let text = ansi_to_tui::IntoText::into_text(&state.preview_content).unwrap_or_default();
    let height = text.lines.len() as u16;
    let visible_height = state.preview_area_height.saturating_sub(2);
    let scroll_offset = state.preview_scroll_offset;

    match key.code {
        KeyCode::Esc => {
            state.copy_mode = None;
            state.preview_selection = None;
            return None;
        }
        KeyCode::Char('h') => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.pending_g = false;
            copy.pending_z = false;
            copy.cursor.col = copy.cursor.col.saturating_sub(1);
        }
        KeyCode::Char('l') => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.pending_g = false;
            copy.pending_z = false;
            let line_len = line_char_count(&text, copy.cursor.row);
            if line_len > 0 && copy.cursor.col < line_len.saturating_sub(1) {
                copy.cursor.col += 1;
            }
        }
        KeyCode::Char('j') => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.pending_g = false;
            copy.pending_z = false;
            if height > 0 && copy.cursor.row < height.saturating_sub(1) {
                copy.cursor.row += 1;
                copy.cursor.col = clamp_col(&text, copy.cursor.row, copy.cursor.col);
            }
        }
        KeyCode::Char('k') => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.pending_g = false;
            copy.pending_z = false;
            if copy.cursor.row > 0 {
                copy.cursor.row -= 1;
                copy.cursor.col = clamp_col(&text, copy.cursor.row, copy.cursor.col);
            }
        }
        KeyCode::Char('0') => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.pending_g = false;
            copy.pending_z = false;
            copy.cursor.col = 0;
        }
        KeyCode::Char('$') => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.pending_g = false;
            copy.pending_z = false;
            let line_len = line_char_count(&text, copy.cursor.row);
            copy.cursor.col = if line_len > 0 { line_len.saturating_sub(1) } else { 0 };
        }
        KeyCode::Char('w') => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.pending_g = false;
            copy.pending_z = false;
            let row = copy.cursor.row as usize;
            if row < text.lines.len() {
                let plain: String = text.lines[row].spans.iter().map(|s| s.content.as_ref()).collect();
                let chars: Vec<char> = plain.chars().collect();
                let mut pos = copy.cursor.col as usize;
                // Skip current word (non-whitespace)
                while pos < chars.len() && !chars[pos].is_whitespace() {
                    pos += 1;
                }
                // Skip whitespace
                while pos < chars.len() && chars[pos].is_whitespace() {
                    pos += 1;
                }
                if pos < chars.len() {
                    copy.cursor.col = pos as u16;
                } else if height > 0 && copy.cursor.row < height.saturating_sub(1) {
                    copy.cursor.row += 1;
                    copy.cursor.col = 0;
                }
            }
        }
        KeyCode::Char('e') => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.pending_g = false;
            copy.pending_z = false;
            let row = copy.cursor.row as usize;
            if row < text.lines.len() {
                let plain: String = text.lines[row].spans.iter().map(|s| s.content.as_ref()).collect();
                let chars: Vec<char> = plain.chars().collect();
                let mut pos = copy.cursor.col as usize;
                if pos + 1 < chars.len() {
                    pos += 1;
                    // Skip whitespace
                    while pos < chars.len() && chars[pos].is_whitespace() {
                        pos += 1;
                    }
                    // Skip non-whitespace (the word)
                    while pos + 1 < chars.len() && !chars[pos + 1].is_whitespace() {
                        pos += 1;
                    }
                    copy.cursor.col = pos as u16;
                } else if height > 0 && copy.cursor.row < height.saturating_sub(1) {
                    // Move to next line, find end of first word
                    copy.cursor.row += 1;
                    let next_row = copy.cursor.row as usize;
                    if next_row < text.lines.len() {
                        let next_plain: String = text.lines[next_row].spans.iter().map(|s| s.content.as_ref()).collect();
                        let next_chars: Vec<char> = next_plain.chars().collect();
                        let mut npos = 0usize;
                        // Skip leading whitespace
                        while npos < next_chars.len() && next_chars[npos].is_whitespace() {
                            npos += 1;
                        }
                        // Skip to end of word
                        while npos + 1 < next_chars.len() && !next_chars[npos + 1].is_whitespace() {
                            npos += 1;
                        }
                        copy.cursor.col = npos as u16;
                    } else {
                        copy.cursor.col = 0;
                    }
                }
            }
        }
        KeyCode::Char('b') => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.pending_g = false;
            copy.pending_z = false;
            let row = copy.cursor.row as usize;
            if row < text.lines.len() {
                let plain: String = text.lines[row].spans.iter().map(|s| s.content.as_ref()).collect();
                let chars: Vec<char> = plain.chars().collect();
                let mut pos = copy.cursor.col as usize;
                if pos > 0 {
                    pos -= 1;
                    // Skip whitespace going left
                    while pos > 0 && chars[pos].is_whitespace() {
                        pos -= 1;
                    }
                    // Skip non-whitespace going left (the word)
                    while pos > 0 && !chars[pos - 1].is_whitespace() {
                        pos -= 1;
                    }
                    copy.cursor.col = pos as u16;
                } else if copy.cursor.row > 0 {
                    copy.cursor.row -= 1;
                    let prev_len = line_char_count(&text, copy.cursor.row);
                    copy.cursor.col = if prev_len > 0 { prev_len.saturating_sub(1) } else { 0 };
                }
            }
        }
        KeyCode::Char('H') => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.pending_g = false;
            copy.pending_z = false;
            copy.cursor.row = scroll_offset;
            copy.cursor.col = clamp_col(&text, copy.cursor.row, copy.cursor.col);
        }
        KeyCode::Char('L') => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.pending_g = false;
            copy.pending_z = false;
            let last_visible = if height > 0 {
                (scroll_offset + visible_height.saturating_sub(1)).min(height.saturating_sub(1))
            } else {
                0
            };
            copy.cursor.row = last_visible;
            copy.cursor.col = clamp_col(&text, copy.cursor.row, copy.cursor.col);
        }
        KeyCode::Char('g') => {
            let copy = state.copy_mode.as_mut().unwrap();
            if copy.pending_g {
                copy.pending_g = false;
                copy.pending_z = false;
                copy.cursor.row = 0;
                copy.cursor.col = clamp_col(&text, 0, copy.cursor.col);
            } else {
                copy.pending_g = true;
                return None;
            }
        }
        KeyCode::Char('G') => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.pending_g = false;
            copy.pending_z = false;
            if height > 0 {
                copy.cursor.row = height.saturating_sub(1);
                copy.cursor.col = clamp_col(&text, copy.cursor.row, copy.cursor.col);
            }
        }
        KeyCode::Char('v') => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.pending_g = false;
            copy.pending_z = false;
            if copy.anchor.is_some() {
                copy.anchor = None;
            } else {
                copy.anchor = Some(ContentPosition {
                    row: copy.cursor.row,
                    col: copy.cursor.col,
                });
            }
        }
        KeyCode::Char('y') => {
            let has_selection = state.preview_selection.is_some();
            let has_anchor = state.copy_mode.as_ref().map_or(false, |c| c.anchor.is_some());

            if has_anchor && has_selection {
                let sel = state.preview_selection.as_ref().unwrap();
                let selected_text = selection::extract_selected_text(&text, sel);
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    let _ = clipboard.set_text(&selected_text);
                    state.toast_message = Some("Copied!".to_string());
                    state.toast_deadline = Some(
                        std::time::Instant::now() + std::time::Duration::from_millis(1500),
                    );
                }
            }
            state.copy_mode = None;
            state.preview_selection = None;
            return None;
        }
        KeyCode::Char('/') => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.pending_g = false;
            copy.pending_z = false;
            copy.search_active = true;
            copy.search_direction = SearchDirection::Forward;
            return None;
        }
        KeyCode::Char('?') => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.pending_g = false;
            copy.pending_z = false;
            copy.search_active = true;
            copy.search_direction = SearchDirection::Backward;
            return None;
        }
        KeyCode::Char('n') => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.pending_g = false;
            copy.pending_z = false;
            if !copy.search_matches.is_empty() {
                let forward = copy.search_direction == SearchDirection::Forward;
                let next_index = match copy.current_match_index {
                    Some(i) => {
                        if forward {
                            (i + 1) % copy.search_matches.len()
                        } else if i == 0 {
                            copy.search_matches.len() - 1
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                copy.current_match_index = Some(next_index);
                copy.cursor.row = copy.search_matches[next_index].row;
                copy.cursor.col = copy.search_matches[next_index].col;
            }
        }
        KeyCode::Char('N') => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.pending_g = false;
            copy.pending_z = false;
            if !copy.search_matches.is_empty() {
                let forward = copy.search_direction == SearchDirection::Forward;
                let prev_index = match copy.current_match_index {
                    Some(i) => {
                        if forward {
                            if i == 0 { copy.search_matches.len() - 1 } else { i - 1 }
                        } else {
                            (i + 1) % copy.search_matches.len()
                        }
                    }
                    None => copy.search_matches.len() - 1,
                };
                copy.current_match_index = Some(prev_index);
                copy.cursor.row = copy.search_matches[prev_index].row;
                copy.cursor.col = copy.search_matches[prev_index].col;
            }
        }
        KeyCode::Char('z') => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.pending_g = false;
            if copy.pending_z {
                copy.pending_z = false;
                let visible_height = state.preview_area_height.saturating_sub(2);
                state.preview_scroll_offset = copy.cursor.row.saturating_sub(visible_height / 2);
                state.preview_is_sticky_bottom = false;
                return None;
            } else {
                copy.pending_z = true;
                return None;
            }
        }
        _ => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.pending_g = false;
            copy.pending_z = false;
            return None;
        }
    }

    ensure_cursor_visible(state);
    sync_selection(state);
    None
}

pub fn handle_copy_mode_search_input(state: &mut AppState, key: KeyEvent) -> Option<Action> {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.search_active = false;
            copy.search_query.clear();
            copy.search_cursor = 0;
            copy.search_matches.clear();
            copy.current_match_index = None;
        }
        (KeyCode::Enter, _) => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.search_active = false;
            if !copy.search_query.is_empty() {
                let query = copy.search_query.clone();
                let text =
                    ansi_to_tui::IntoText::into_text(&state.preview_content).unwrap_or_default();
                let matches = find_matches(&text, &query);
                let cursor_row = state.copy_mode.as_ref().unwrap().cursor.row;
                let cursor_col = state.copy_mode.as_ref().unwrap().cursor.col;
                let search_direction = state.copy_mode.as_ref().unwrap().search_direction;
                let found_index = if search_direction == SearchDirection::Forward {
                    matches.iter().position(|m| {
                        m.row > cursor_row || (m.row == cursor_row && m.col >= cursor_col)
                    })
                } else {
                    matches.iter().rposition(|m| {
                        m.row < cursor_row || (m.row == cursor_row && m.col <= cursor_col)
                    })
                };
                let match_index = found_index.or_else(|| if matches.is_empty() { None } else {
                    if search_direction == SearchDirection::Forward { Some(0) } else { Some(matches.len() - 1) }
                });
                let copy = state.copy_mode.as_mut().unwrap();
                copy.search_matches = matches;
                copy.current_match_index = match_index;
                if let Some(idx) = match_index {
                    copy.cursor.row = copy.search_matches[idx].row;
                    copy.cursor.col = copy.search_matches[idx].col;
                }
            }
            ensure_cursor_visible(state);
            sync_selection(state);
        }
        (KeyCode::Char('a'), KeyModifiers::CONTROL) => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.search_cursor = 0;
        }
        (KeyCode::Char('e'), KeyModifiers::CONTROL) => {
            let copy = state.copy_mode.as_mut().unwrap();
            copy.search_cursor = copy.search_query.chars().count();
        }
        (KeyCode::Char('u'), KeyModifiers::CONTROL) | (KeyCode::Backspace, KeyModifiers::SUPER) => {
            let copy = state.copy_mode.as_mut().unwrap();
            let byte_offset = copy
                .search_query
                .char_indices()
                .nth(copy.search_cursor)
                .map(|(i, _)| i)
                .unwrap_or(copy.search_query.len());
            copy.search_query.drain(..byte_offset);
            copy.search_cursor = 0;
        }
        (KeyCode::Char('k'), KeyModifiers::CONTROL) => {
            let copy = state.copy_mode.as_mut().unwrap();
            let byte_offset = copy
                .search_query
                .char_indices()
                .nth(copy.search_cursor)
                .map(|(i, _)| i)
                .unwrap_or(copy.search_query.len());
            copy.search_query.truncate(byte_offset);
        }
        (KeyCode::Char('b'), KeyModifiers::CONTROL) | (KeyCode::Left, KeyModifiers::NONE) => {
            let copy = state.copy_mode.as_mut().unwrap();
            if copy.search_cursor > 0 {
                copy.search_cursor -= 1;
            }
        }
        (KeyCode::Char('f'), KeyModifiers::CONTROL) | (KeyCode::Right, KeyModifiers::NONE) => {
            let copy = state.copy_mode.as_mut().unwrap();
            let len = copy.search_query.chars().count();
            if copy.search_cursor < len {
                copy.search_cursor += 1;
            }
        }
        (KeyCode::Left, KeyModifiers::ALT) => {
            let copy = state.copy_mode.as_mut().unwrap();
            let chars: Vec<char> = copy.search_query.chars().collect();
            let mut pos = copy.search_cursor;
            while pos > 0 && chars[pos - 1].is_whitespace() {
                pos -= 1;
            }
            while pos > 0 && !chars[pos - 1].is_whitespace() {
                pos -= 1;
            }
            copy.search_cursor = pos;
        }
        (KeyCode::Right, KeyModifiers::ALT) => {
            let copy = state.copy_mode.as_mut().unwrap();
            let chars: Vec<char> = copy.search_query.chars().collect();
            let len = chars.len();
            let mut pos = copy.search_cursor;
            while pos < len && !chars[pos].is_whitespace() {
                pos += 1;
            }
            while pos < len && chars[pos].is_whitespace() {
                pos += 1;
            }
            copy.search_cursor = pos;
        }
        (KeyCode::Backspace, KeyModifiers::ALT) => {
            let copy = state.copy_mode.as_mut().unwrap();
            let chars: Vec<char> = copy.search_query.chars().collect();
            let mut pos = copy.search_cursor;
            while pos > 0 && chars[pos - 1].is_whitespace() {
                pos -= 1;
            }
            while pos > 0 && !chars[pos - 1].is_whitespace() {
                pos -= 1;
            }
            let start_byte = copy
                .search_query
                .char_indices()
                .nth(pos)
                .map(|(i, _)| i)
                .unwrap_or(copy.search_query.len());
            let end_byte = copy
                .search_query
                .char_indices()
                .nth(copy.search_cursor)
                .map(|(i, _)| i)
                .unwrap_or(copy.search_query.len());
            copy.search_query.drain(start_byte..end_byte);
            copy.search_cursor = pos;
        }
        (KeyCode::Backspace, _) => {
            let copy = state.copy_mode.as_mut().unwrap();
            if copy.search_cursor > 0 {
                let byte_at_cursor = copy
                    .search_query
                    .char_indices()
                    .nth(copy.search_cursor - 1)
                    .map(|(i, _)| i)
                    .unwrap_or(copy.search_query.len());
                let next_byte = copy
                    .search_query
                    .char_indices()
                    .nth(copy.search_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(copy.search_query.len());
                copy.search_query.drain(byte_at_cursor..next_byte);
                copy.search_cursor -= 1;
            }
        }
        (KeyCode::Delete, _) => {
            let copy = state.copy_mode.as_mut().unwrap();
            let len = copy.search_query.chars().count();
            if copy.search_cursor < len {
                let byte_at_cursor = copy
                    .search_query
                    .char_indices()
                    .nth(copy.search_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(copy.search_query.len());
                let next_byte = copy
                    .search_query
                    .char_indices()
                    .nth(copy.search_cursor + 1)
                    .map(|(i, _)| i)
                    .unwrap_or(copy.search_query.len());
                copy.search_query.drain(byte_at_cursor..next_byte);
            }
        }
        (KeyCode::Char(c), _) => {
            let copy = state.copy_mode.as_mut().unwrap();
            let byte_offset = copy
                .search_query
                .char_indices()
                .nth(copy.search_cursor)
                .map(|(i, _)| i)
                .unwrap_or(copy.search_query.len());
            copy.search_query.insert(byte_offset, c);
            copy.search_cursor += 1;
        }
        _ => {}
    }
    None
}

fn find_matches(text: &Text, query: &str) -> Vec<SearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }
    let query_lower = query.to_lowercase();
    let match_char_len = query.chars().count() as u16;
    let mut matches = Vec::new();

    for (row_idx, line) in text.lines.iter().enumerate() {
        let plain: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        let plain_lower = plain.to_lowercase();

        let mut search_start = 0usize;
        while let Some(byte_pos) = plain_lower[search_start..].find(&query_lower) {
            let abs_byte_pos = search_start + byte_pos;
            let col = plain[..abs_byte_pos].chars().count() as u16;
            matches.push(SearchMatch {
                row: row_idx as u16,
                col,
                len: match_char_len,
            });
            let advance = if query_lower.is_empty() { 1 } else { query_lower.len() };
            search_start = abs_byte_pos + advance;
            if search_start > plain_lower.len() {
                break;
            }
        }
    }

    matches
}

pub fn apply_cursor_highlight(
    text: &mut Text,
    row: u16,
    col: u16,
    scroll_offset: u16,
    visible_height: u16,
) {
    if row < scroll_offset || row >= scroll_offset + visible_height {
        return;
    }
    if (row as usize) >= text.lines.len() {
        return;
    }
    let line = &mut text.lines[row as usize];
    let line_len: u16 = line.spans.iter().map(|s| s.content.chars().count() as u16).sum();
    if col >= line_len {
        line.spans.push(Span::styled(
            " ",
            Style::default().add_modifier(Modifier::REVERSED),
        ));
    } else {
        selection::highlight_spans_in_range(
            &mut line.spans,
            col,
            col + 1,
            Style::default().add_modifier(Modifier::REVERSED),
        );
    }
}

pub fn apply_search_highlights(
    text: &mut Text,
    matches: &[SearchMatch],
    current_match_index: Option<usize>,
    scroll_offset: u16,
    visible_height: u16,
) {
    let normal_style = Style::default()
        .bg(Color::Rgb(0x88, 0x88, 0x00))
        .fg(Color::Black);
    let current_style = Style::default()
        .bg(Color::Rgb(0xFF, 0xFF, 0x00))
        .fg(Color::Black);

    for (idx, m) in matches.iter().enumerate() {
        if m.row < scroll_offset || m.row >= scroll_offset + visible_height {
            continue;
        }
        if (m.row as usize) >= text.lines.len() {
            continue;
        }
        let style = if current_match_index == Some(idx) {
            current_style
        } else {
            normal_style
        };
        selection::highlight_spans_in_range(
            &mut text.lines[m.row as usize].spans,
            m.col,
            m.col + m.len,
            style,
        );
    }
}
