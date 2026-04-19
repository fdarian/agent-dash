# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

Agent Dash is a Rust TUI application for managing and monitoring agent sessions running in tmux. Built with ratatui, crossterm, and tokio.

## CI / Quality

- CI runs `cargo clippy -- -D warnings` — all clippy warnings are errors
- CI runs `cargo test`
- Run `/verify` to check both locally before pushing

## Architecture

- **Event loop** (`app.rs`): crossterm event stream + background tmux polling via mpsc channel
- **Preview pipeline**: FIFO-based pipe watching (`pipe_pane.rs`) with fallback to `tmux capture-pane` polling, ANSI codes converted via `ansi-to-tui` crate
- **Copy mode** (`copy_mode.rs`): vim-like motions, search, and selection on frozen preview content
- **Session grouping** (`session.rs`): sessions grouped by tmux session name, with collapsible groups and hidden section
- **Persistence** (`state.rs`, `cache.rs`): state saved to `~/.config/agent-dash/state.json`, sessions cached to `~/.config/agent-dash/cache/`

## Documentation Site

- `apps/docs/` — Fumadocs site (Next.js 16, Tailwind 4)
- See `apps/docs/CLAUDE.md` for docs-specific guidance
- See `apps/docs/notes/design-system.md` for the design system

## Non-obvious details

- Session "active" status is detected via braille Unicode range (0x2800-0x28FF) in pane title — Claude CLI sets these
- Prompt state (Plan/Ask) is parsed from the last non-empty line of pane content
- Claude process detection uses recursive pgrep on tmux pane child processes
- FIFO monitoring creates temp files at `/tmp/agent-dash-{pid}-preview.fifo` with O_NONBLOCK
