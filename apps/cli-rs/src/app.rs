use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::prelude::*;
use std::time::Duration;

use crate::ui;

pub struct AppState {
    pub should_quit: bool,
    pub exit_on_switch: bool,
}

impl AppState {
    fn new(exit_on_switch: bool) -> Self {
        Self {
            should_quit: false,
            exit_on_switch,
        }
    }
}

pub async fn run(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>, exit_on_switch: bool) -> Result<()> {
    let mut state = AppState::new(exit_on_switch);

    loop {
        terminal.draw(|frame| ui::render(frame, &state))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                handle_key_event(&mut state, key);
            }
        }

        if state.should_quit {
            break;
        }
    }

    Ok(())
}

fn handle_key_event(state: &mut AppState, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => state.should_quit = true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.should_quit = true;
        }
        _ => {}
    }
}
