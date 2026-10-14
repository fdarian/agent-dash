use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::theme::Palette;
use crate::app::view_config::ViewConfigOverlay;
use crate::app::{AppState, Focus};

pub(super) struct BarEntry {
    key: &'static str,
    desc: &'static str,
}

impl BarEntry {
    pub(super) fn new(key: &'static str, desc: &'static str) -> BarEntry {
        BarEntry { key, desc }
    }
}

fn current_entries(state: &AppState) -> Vec<BarEntry> {
    if state.pending_confirm_target.is_some() {
        return vec![
            BarEntry {
                key: "Enter",
                desc: "confirm",
            },
            BarEntry {
                key: "Esc",
                desc: "cancel",
            },
        ];
    }
    if let Some(overlay) = state.view_config {
        return match overlay {
            ViewConfigOverlay::Rows { .. } => vec![
                BarEntry {
                    key: "j/k",
                    desc: "navigate",
                },
                BarEntry {
                    key: "1-3",
                    desc: "edit setting",
                },
                BarEntry {
                    key: "Enter",
                    desc: "open",
                },
                BarEntry {
                    key: "Esc",
                    desc: "close",
                },
            ],
            ViewConfigOverlay::Selector { .. } => vec![
                BarEntry {
                    key: "j/k",
                    desc: "navigate",
                },
                BarEntry {
                    key: "Enter",
                    desc: "apply",
                },
                BarEntry {
                    key: "Esc",
                    desc: "back",
                },
            ],
        };
    }
    if state.show_help {
        return vec![
            BarEntry {
                key: "Esc/?",
                desc: "close",
            },
            BarEntry {
                key: "/",
                desc: "filter",
            },
        ];
    }
    if state.copy_mode.is_some() {
        return vec![
            BarEntry {
                key: "Esc",
                desc: "exit",
            },
            BarEntry {
                key: "v",
                desc: "select",
            },
            BarEntry {
                key: "y",
                desc: "yank",
            },
            BarEntry {
                key: "/ ?",
                desc: "search",
            },
            BarEntry {
                key: "n N",
                desc: "next/prev",
            },
        ];
    }
    if state.session_filter_active {
        return vec![
            BarEntry {
                key: "Enter",
                desc: "confirm",
            },
            BarEntry {
                key: "Esc",
                desc: "cancel",
            },
        ];
    }
    match state.focus {
        Focus::Sessions => vec![
            BarEntry {
                key: "j/k",
                desc: "navigate",
            },
            BarEntry {
                key: "o",
                desc: "open/resume",
            },
            BarEntry {
                key: "n",
                desc: "note",
            },
            BarEntry {
                key: "r",
                desc: "mark read",
            },
            BarEntry {
                key: "c",
                desc: "create",
            },
            BarEntry {
                key: "a",
                desc: "archive",
            },
            BarEntry {
                key: "u",
                desc: "undo",
            },
            BarEntry {
                key: "x",
                desc: "close",
            },
            BarEntry {
                key: "S",
                desc: "view",
            },
            BarEntry {
                key: "?",
                desc: "help",
            },
        ],
        Focus::Preview => vec![
            BarEntry {
                key: "j/k",
                desc: "scroll",
            },
            BarEntry {
                key: "v",
                desc: "copy mode",
            },
            BarEntry {
                key: "o",
                desc: "switch pane",
            },
            BarEntry {
                key: "/",
                desc: "search",
            },
            BarEntry {
                key: "?",
                desc: "help",
            },
        ],
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    render_entries(frame, area, &state.theme, &current_entries(state));
}

pub(super) fn render_entries(frame: &mut Frame, area: Rect, theme: &Palette, entries: &[BarEntry]) {
    let mut spans = Vec::new();
    spans.push(Span::raw(" "));
    for (i, entry) in entries.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", Style::default().fg(theme.bg_selected)));
        }
        spans.push(Span::styled(
            entry.key,
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            entry.desc,
            Style::default().fg(theme.text_subtle),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
