use ratatui::prelude::*;

pub struct ContentPosition {
    pub row: u16,
    pub col: u16,
}

pub struct PreviewSelection {
    pub anchor: ContentPosition,
    pub cursor: ContentPosition,
    pub is_dragging: bool,
}

const SELECTION_BG: Style = Style::new().bg(Color::Rgb(0x44, 0x44, 0x88)).fg(Color::White);

pub fn mouse_to_content_position(
    mouse_col: u16,
    mouse_row: u16,
    preview_pane_area: Rect,
    scroll_offset: u16,
) -> Option<ContentPosition> {
    let inner_x = preview_pane_area.x + 1;
    let inner_y = preview_pane_area.y + 1;
    let inner_width = preview_pane_area.width.saturating_sub(2);
    let inner_height = preview_pane_area.height.saturating_sub(2);

    if mouse_col < inner_x
        || mouse_row < inner_y
        || mouse_col >= inner_x + inner_width
        || mouse_row >= inner_y + inner_height
    {
        return None;
    }

    Some(ContentPosition {
        col: mouse_col - inner_x,
        row: (mouse_row - inner_y) + scroll_offset,
    })
}

pub fn ordered_bounds(selection: &PreviewSelection) -> (u16, u16, u16, u16) {
    let anchor_first = selection.anchor.row < selection.cursor.row
        || (selection.anchor.row == selection.cursor.row
            && selection.anchor.col <= selection.cursor.col);

    if anchor_first {
        (
            selection.anchor.row,
            selection.anchor.col,
            selection.cursor.row,
            selection.cursor.col,
        )
    } else {
        (
            selection.cursor.row,
            selection.cursor.col,
            selection.anchor.row,
            selection.anchor.col,
        )
    }
}

pub fn extract_selected_text(text: &Text, selection: &PreviewSelection) -> String {
    let (start_row, start_col, end_row, end_col) = ordered_bounds(selection);

    let line_count = text.lines.len() as u16;
    let clamped_start = start_row.min(line_count.saturating_sub(1));
    let clamped_end = end_row.min(line_count.saturating_sub(1));

    let mut result = Vec::new();

    for row in clamped_start..=clamped_end {
        let line = &text.lines[row as usize];
        let plain: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        let segment = if row == start_row && row == end_row {
            let start = start_col as usize;
            let end = end_col as usize;
            plain.chars().skip(start).take(end.saturating_sub(start)).collect::<String>()
        } else if row == start_row {
            plain.chars().skip(start_col as usize).collect::<String>()
        } else if row == end_row {
            plain.chars().take(end_col as usize).collect::<String>()
        } else {
            plain
        };

        result.push(segment);
    }

    result.join("\n")
}

pub fn apply_selection_highlight(
    text: &mut Text,
    selection: &PreviewSelection,
    scroll_offset: u16,
    visible_height: u16,
) {
    let (start_row, start_col, end_row, end_col) = ordered_bounds(selection);

    let visible_start = scroll_offset;
    let visible_end = scroll_offset + visible_height;

    for content_row in start_row..=end_row {
        if content_row < visible_start || content_row >= visible_end {
            continue;
        }

        if (content_row as usize) >= text.lines.len() {
            continue;
        }

        let sel_start = if content_row == start_row { start_col } else { 0 };
        let sel_end = if content_row == end_row { end_col } else { u16::MAX };

        highlight_spans_in_range(&mut text.lines[content_row as usize].spans, sel_start, sel_end);
    }
}

fn highlight_spans_in_range(spans: &mut Vec<Span>, sel_start: u16, sel_end: u16) {
    let mut col: u16 = 0;
    let mut i = 0;

    while i < spans.len() {
        let span_char_count = spans[i].content.chars().count() as u16;
        let span_start = col;
        let span_end = col + span_char_count;

        if span_end <= sel_start || span_start >= sel_end {
            col = span_end;
            i += 1;
            continue;
        }

        if span_start >= sel_start && span_end <= sel_end {
            spans[i].style = spans[i].style.patch(SELECTION_BG);
            col = span_end;
            i += 1;
            continue;
        }

        let original_style = spans[i].style;
        let content = spans[i].content.to_string();

        let overlap_start = sel_start.max(span_start) - span_start;
        let overlap_end = sel_end.min(span_end) - span_start;

        let mut parts: Vec<Span> = Vec::new();

        if overlap_start > 0 {
            let before = chars_slice(&content, 0, overlap_start as usize);
            parts.push(Span::styled(before, original_style));
        }

        let selected = chars_slice(&content, overlap_start as usize, overlap_end as usize);
        parts.push(Span::styled(selected, original_style.patch(SELECTION_BG)));

        if overlap_end < span_char_count {
            let after = chars_slice(&content, overlap_end as usize, span_char_count as usize);
            parts.push(Span::styled(after, original_style));
        }

        let parts_len = parts.len();
        spans.splice(i..=i, parts);

        col = span_end;
        i += parts_len;
    }
}

fn chars_slice(s: &str, start: usize, end: usize) -> String {
    s.chars().skip(start).take(end - start).collect()
}
