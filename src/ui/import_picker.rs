use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use super::bottom_bar::{render_entries, BarEntry};
use super::session_list::truncate_or_pad;
use super::theme::Palette;
use crate::app::import_picker::{ImportPicker, SOURCE_TABS};
use crate::sessions::{basename, display_title};
use crate::time_fmt::format_relative_time;

const SOURCE_WIDTH: usize = 7;
const FOLDER_WIDTH: usize = 18;
const AGE_WIDTH: usize = 4;

fn hint_entries(picker: &ImportPicker) -> Vec<BarEntry> {
    if picker.editing_query {
        return vec![
            BarEntry::new("Enter", "keep filter"),
            BarEntry::new("Esc", "clear filter"),
        ];
    }
    vec![
        BarEntry::new("j/k", "move"),
        BarEntry::new("Tab", "source"),
        BarEntry::new("Space", "toggle"),
        BarEntry::new("a", "select all shown"),
        BarEntry::new("n", "select none"),
        BarEntry::new("/", "filter"),
        BarEntry::new("Enter", "import"),
        BarEntry::new(
            "Esc",
            if picker.query.is_empty() {
                "quit"
            } else {
                "clear filter"
            },
        ),
    ]
}

/// `Import sessions · All · CLI · Desktop · 2 of 9 selected`, the active source highlighted.
fn header<'a>(theme: &Palette, picker: &ImportPicker) -> Line<'a> {
    let separator = Span::styled(" · ", Style::default().fg(theme.text_subtle));
    let mut spans = vec![Span::raw(" Import sessions"), separator.clone()];
    for (index, tab) in SOURCE_TABS.iter().enumerate() {
        if index > 0 {
            spans.push(separator.clone());
        }
        let label = tab.map_or("All", |source| source.label());
        let style = if *tab == picker.source {
            Style::default()
                .fg(theme.selected_fg)
                .bg(theme.bg_selected)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_subtle)
        };
        spans.push(Span::styled(format!(" {label} "), style));
    }
    spans.push(separator);
    spans.push(Span::raw(format!(
        "{} of {} selected ",
        picker.selected.len(),
        picker.candidates.len()
    )));
    if picker.skipped > 0 {
        spans.push(Span::raw(format!(
            "· {} unreadable skipped ",
            picker.skipped
        )));
    }
    Line::from(spans)
}

pub fn render_scanning(frame: &mut Frame, theme: &Palette) {
    let area = frame.area();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.primary))
        .title(" Import sessions ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Line::from(" Scanning...").fg(theme.text_subtle), inner);
}

pub fn render(frame: &mut Frame, theme: &Palette, picker: &ImportPicker) {
    let [list_area, hint_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());
    render_list(frame, theme, picker, list_area);
    render_entries(frame, hint_area, theme, &hint_entries(picker));
}

fn render_list(frame: &mut Frame, theme: &Palette, picker: &ImportPicker, area: Rect) {
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.primary))
        .title(header(theme, picker));
    if picker.editing_query || !picker.query.is_empty() {
        let (text, color) = if picker.query.is_empty() {
            ("Type to filter...", theme.text_muted)
        } else {
            (picker.query.as_str(), theme.text)
        };
        block = block.title_bottom(Line::from(vec![
            Span::styled("/", Style::default().fg(theme.text_subtle)),
            Span::styled(text.to_string(), Style::default().fg(color)),
            Span::raw(" "),
        ]));
        if picker.editing_query {
            frame.set_cursor_position((
                area.x + 2 + picker.query_cursor as u16,
                area.y + area.height - 1,
            ));
        }
    }

    if picker.visible.is_empty() {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let text = if picker.candidates.is_empty() {
            " No untracked sessions found"
        } else {
            " No matching sessions"
        };
        frame.render_widget(Line::from(text).fg(theme.text_subtle), inner);
        return;
    }

    let inner_width = area.width.saturating_sub(2) as usize;
    let now_epoch = chrono::Local::now().timestamp();
    let items: Vec<ListItem> = picker
        .visible
        .iter()
        .enumerate()
        .map(|(row, index)| {
            let candidate = &picker.candidates[*index];
            let is_cursor = row == picker.cursor;
            let (primary, secondary, row_style) = if is_cursor {
                (
                    Style::default().fg(theme.selected_fg),
                    Style::default().fg(theme.selected_dim_fg),
                    Style::default().bg(theme.bg_selected),
                )
            } else {
                (
                    Style::default().fg(theme.text),
                    Style::default().fg(theme.text_muted),
                    Style::default(),
                )
            };
            let mark = if picker.is_selected(candidate) {
                "[x]"
            } else {
                "[ ]"
            };
            let age = format_relative_time(now_epoch, candidate.last_activity as i64);
            // Leading space, the fixed columns and the single spaces between them.
            let title_width = inner_width
                .saturating_sub(1 + 3 + 1 + SOURCE_WIDTH + 1 + 1 + FOLDER_WIDTH + 1 + AGE_WIDTH);
            let line = Line::from(vec![
                Span::styled(format!(" {mark} "), primary),
                Span::styled(
                    format!(
                        "{} ",
                        truncate_or_pad(candidate.source.label(), SOURCE_WIDTH)
                    ),
                    secondary,
                ),
                Span::styled(
                    format!(
                        "{} ",
                        truncate_or_pad(
                            &display_title(&candidate.title, &candidate.session_id),
                            title_width
                        )
                    ),
                    primary,
                ),
                Span::styled(
                    format!(
                        "{} {:>age_width$}",
                        truncate_or_pad(&basename(&candidate.cwd), FOLDER_WIDTH),
                        age,
                        age_width = AGE_WIDTH
                    ),
                    secondary,
                ),
            ]);
            ListItem::new(line.style(row_style))
        })
        .collect();
    let widget = List::new(items).block(block);
    let mut list_state = ListState::default().with_selected(Some(picker.cursor));
    frame.render_stateful_widget(widget, area, &mut list_state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::import::{ImportCandidate, ImportSource, ScanOutcome};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;

    fn screen(picker: &ImportPicker) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 8)).unwrap();
        terminal
            .draw(|frame| render(frame, &Palette::dark(), picker))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn picker() -> ImportPicker {
        let candidates = ["one", "two"]
            .iter()
            .map(|title| ImportCandidate {
                session_id: title.to_string(),
                source: ImportSource::Cli,
                title: title.to_string(),
                cwd: "/work/app".to_string(),
                last_activity: 1,
            })
            .collect();
        ImportPicker::new(ScanOutcome {
            candidates,
            skipped: 0,
        })
    }

    #[test]
    fn title_counts_selected_rows_and_footer_lists_the_keys() {
        let mut picker = picker();
        let before = screen(&picker);
        assert!(before.contains("0 of 2 selected"), "{before}");
        assert!(before.contains("All") && before.contains("CLI") && before.contains("Desktop"));
        for key in ["Tab", "Space", "a", "n", "/", "Enter", "Esc"] {
            assert!(before.contains(key), "missing {key}: {before}");
        }
        picker.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert!(screen(&picker).contains("2 of 2 selected"));
    }
}
