# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

Agent Dash is a Rust TUI application for managing and monitoring Claude Code sessions, live in tmux or closed. Built with ratatui, crossterm, and tokio.

## CI / Quality

- CI runs `cargo clippy -- -D warnings` — all clippy warnings are errors
- CI runs `cargo test`
- Run `/verify` to check both locally before pushing

## Architecture

- **Sessions** (`src/sessions/`): a session is keyed by Claude `sessionId`, not by pane. `Session` = persisted `SessionRecord` + `Option<LiveSession>`
  - `record.rs` — the store (`~/.config/agent-dash/sessions.json`): atomic writes; several instances coexist because records merge per field group (archive state, unread state, each with its own `*_changed_at` epoch-ms stamp, newest wins; metadata by larger `last_activity`) and deletions persist as 30-day tombstones. `sync` only re-reads when the file's mtime/len changed
  - `live.rs` — the per-tick live overlay and the pure `track` function that turns two consecutive ticks into store mutations
  - `view.rs` — status/activity filters and the three groupings (state, folder, date); pure and unit-tested
  - `repo.rs` — `RepoRoots`, a never-invalidated per-cwd memo of the repo root found by reading `.git` (dir, or `gitdir:` file plus `commondir`); no `git` process. The root is persisted on the record (`repo_root`, metadata field group) as soon as it resolves, while the cwd still exists, so sessions of a deleted worktree still group with their repo; changing `cwd` clears it for re-resolving
  - `discover.rs` — turns the multiplexer's panes into a `LiveSet` (Claude process detection + pid file via `claude_sessions.rs`)
  - `import.rs` — scanners for existing sessions (see Import below)
- **Folder**: a session's folder is its record's `repo_root` when it has one, else its cwd (`SessionRecord::folder_key`), so a repo's worktrees share one. The Folder grouping's keys and the `c` dialog's folder list both use it
- **Multiplexer** (`src/multiplexer/`): `Multiplexer` trait covers lifecycle only (list panes, focus, spawn, kill, name check) and knows nothing about agents; it must not import `sessions` or `claude_*`. `tmux.rs` is the only implementation. Preview, capture, pipe, resize and copy mode talk to tmux directly
- **App** (`src/app/`): `poll.rs` discovers panes every 2s and sends a `LiveSet` (`None` when tmux is unreachable); `mod.rs` runs `track` on each message, applies the mutations to the store and saves; `actions.rs` handles switch/resume/create; key handling is split per mode under `keys/`, with the view-config overlay in `view_config.rs`
- **List navigation** (`ui/list_nav.rs`): one helper maps j/k, arrows, Ctrl+n/Ctrl+p, Enter, Esc for every list and picker; new lists use it rather than matching keys themselves
- **New session** (`app/new_session.rs` state machine, `ui/new_session.rs` widget, `worktree.rs`): `c` opens a dialog to pick a folder (a typed path or a known folder) and, when the config has `worktree.create`, optionally get a git worktree first. A Mode field right after Folder, present only when the config has `modes`, picks the command the session starts with (`Submission.command`, carried through `Action::CreateSession`, `Message::WorktreeCreated` and `Action::SpawnInWorktree`); mode 0 is the implicit `default` = top-level `command`, and resume always uses `command`. One shared suggestion list follows the focused field (folders, local branches for Name, local+remote for Base); Fields work like zsh menu-complete: each keeps `text` (shown, submitted) and `query` (last typed, what the list is filtered by), Down/Up write the highlighted suggestion into `text` (Up from the first restores the text from before cycling; a prefilled Base has an empty `query`, so its list isn't filtered), Tab only moves focus, Enter submits the texts. An empty Folder resolves to the default folder (the selected session's, else the most recent), shown as a placeholder; typed text must be an existing directory. The state machine does no git itself; a repo loader (branches, worktrees, current branch) is injected. `WorktreeRequest` is `New` (`worktree.create`), `Checkout` (`worktree.checkout`) or `Existing` (open the branch's worktree, no command). `worktree::run` renders the template (shell-quoted; `{{name}}`/`{{base}}`, only `{{name}}` for checkout), runs it with `sh -c` in the folder (the repo root) inside a tokio task, then finds the path with `git worktree list --porcelain`; the result returns as `Message::WorktreeCreated` and `Action::SpawnInWorktree` starts the session. Git stays out of `multiplexer/`
- **Import** (`import_cmd.rs`, `sessions/import.rs`): `agent-dash import` is a subcommand with its own full-screen picker (`app/import_picker.rs` state, `ui/import_picker.rs` widget); `--all` / `--since <1d|3d|7d|30d>` skip the TUI and take `--source <cli|desktop>`; in the picker Tab cycles the same source filter, combined with `/`, while the selection persists across filters. The main app has no import UI, only an empty-list hint. The scan lists Claude sessions not yet in the store from `~/.claude/projects/*/*.jsonl` (CLI) and Claude Desktop's `claude-code-sessions/*/*/local_*.json`, one thread per chunk of files. Desktop wins on overlap and sessions archived in Desktop are skipped. Transcripts are never read whole (`claude_transcript::summarize`): the last 1 MiB gives title, cwd and activity, and only without a custom/ai title the first 64 KiB gives the first user prompt, then `last-prompt`. Only UUID session ids are offered, and `resume_command` refuses non-UUIDs and shell-quotes the id
- **UI state** (`state.rs`): `~/.config/agent-dash/state.json` holds the global view config (status, grouping, last-activity window), the global New session Worktree checkbox (recorded on submit; ignored, not cleared, while `worktree.create` is unset) and per-instance collapsed groups keyed by group key. Keys it doesn't know are kept on save, so versions sharing the file don't strip each other's fields
- **Preview pipeline**: FIFO-based pipe watching (`pipe_pane.rs`) with fallback to `tmux capture-pane` polling, ANSI codes converted via `ansi-to-tui`
- **Copy mode** (`copy_mode.rs`): vim-like motions, search, and selection on frozen preview content

## Documentation Site

- `apps/docs/` — Fumadocs site (Next.js 16, Tailwind 4)
- See `apps/docs/CLAUDE.md` for docs-specific guidance
- See `apps/docs/notes/design-system.md` for the design system

## Internal docs

- [docs/agent-detection.md](docs/agent-detection.md) — how Claude panes are detected and how busy/idle status is determined

## Non-obvious details

- Claude Code is the only agent. Discovery is tmux-pane based: `list-panes` -> claude process detection (recursive pgrep on pane children, `claude_sessions::find_claude_pid`) -> `~/.claude/sessions/<claude_pid>.json` (`claude_sessions.rs`, Claude Code 2.1.119 or later), which supplies session id, cwd, name (the title is the user's name, `nameSource` `user` or absent; else the transcript's custom-title, then ai-title, via `TranscriptCache::title`; else the Claude-generated name, `nameSource` `derived`), status (`busy`; `waiting` on the user counts as Need action like a Plan/Ask prompt; else idle) and start time; `sessions/discover.rs` glues the steps together. A pane without that file is skipped; there is no pane-title fallback
- Closing is classified from the pid file in `track` (`sessions/live.rs`):
  - file missing, or holding another session id -> archive with reason `closed` (delete the record if the session has no transcript; the transcript lookup only happens here)
  - same id, pid dead -> crash: leave it as closed
  - same id, pid alive, or file present but unreadable/malformed -> pending: re-checked each reachable tick for up to 10s (the `Closing` set in `SessionBook`), then left as closed
  - no close transitions on the first tick or while tmux is unreachable
- `autoTrackSessions = false` (config) makes `track` skip inserting newly detected live sessions; only ones on a pane spawned by `c` (`created_panes`, origin `created`) are inserted. Stored sessions keep resuming, auto-archiving and flagging unread, and live panes outside the store aren't shown. `SessionBook::set_auto_track` is fed from the config before every tick, so reloads apply live
- `a` archives with reason `user`; archiving (either kind) clears unread. `u` pops one in-memory undo stack (`app/undo.rs`, 50 deep, not persisted) shared by `a` toggles and view-config changes (`S` overlay options, the `` ` `` active/archived toggle). Undoing `a` goes through `Store::restore_archive`, which puts back the snapshotted archive fields and unread and stamps both change times so it wins merges; undoing a view change restores the previous `ViewConfig`
- An archived session unarchives only when a live process started (pid file `startedAt`, ms) after `archive_changed_at`; appearing in the live set doesn't count, so a discovery blip can't undo an archive and archiving an already-live session sticks
- A deleted id keeps a tombstone (epoch ms) in `sessions.json`; a record only beats it if an intent field changed after the deletion
- Saves: intent changes (insert, archive, unread, delete) save at once; metadata-only changes at most every 10s (flushed on exit). A failed save stays dirty and is retried each tick, with the error toasted once per failure streak
- `TmuxClient::spawn` starts the pane's default interactive shell and types the command with `send-keys` rather than passing a shell-command, so rc-file aliases/env apply and the pane survives the command exiting
- Prompt state (Plan/Ask) is parsed from the last non-empty line of pane content, only for idle live sessions
- FIFO monitoring creates temp files at `/tmp/agent-dash-{pid}-preview.fifo` with O_NONBLOCK
