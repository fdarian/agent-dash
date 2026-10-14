# Agent Detection

Claude Code is the only supported agent. Discovery needs Claude Code 2.1.119 or later, the first release whose `~/.claude/sessions/<pid>.json` has `status`; there is no fallback for older versions.

## Process detection

Every 2 s the poll task (`src/app/poll.rs`) calls `sessions::discover::discover_live`. It asks the multiplexer for its panes (`Multiplexer::list_panes`; the tmux implementation in `src/multiplexer/tmux.rs` runs `tmux list-panes`) and runs `find_claude_pid` (`src/claude_sessions.rs`) on each pane's root pid.

`find_claude_pid` walks the pane's process tree recursively via `pgrep` and inspects each process's executable name (`comm`) and command-line arguments (`args`). A process counts as a Claude session only when:

1. The executable basename is exactly `claude` (case-sensitive, after stripping any leading `-` login-shell prefix).
2. The command has no positional subcommand after the executable — only flags (arguments starting with `-`) are allowed.

Accepted: `claude`, `claude --model opus --agent guide`.
Rejected: `oagent serve` (wrong executable), `claude mcp serve` and `claude doctor` (positional subcommand).

`find_claude_pid` returns the PID of the matched process, which is the join key for the session file below.

## Session file

Claude Code writes `~/.claude/sessions/<claude_pid>.json` for each live process (`src/claude_sessions.rs`). It supplies:

- `sessionId` — the identity of the session. A pane whose process has no readable file is skipped, so it never appears.
- `cwd`, `startedAt`, and `name` (the live title), which Claude only writes for named sessions. Without it the title is the transcript's `custom-title`, then `ai-title` (tail read, cached in `TranscriptCache`); a missing name never overwrites a stored title
- `status` — `busy` marks the session busy, `waiting` means Claude waits on the user (a permission prompt or a question), every other value (`idle`, `shell`, anything new) is idle

Several panes can run the same `sessionId` (a session resumed twice). `LiveSet::collapse` keeps one entry, preferring a busy pane, then the latest start.

Last activity comes from the session transcript (`claude_transcript.rs`), not from tmux, because tmux window activity moves on any output.

## Prompt state (Plan / Ask)

`detect_prompt_state` (`src/sessions/live.rs`) parses the last non-empty line of the visible pane content. The poll task only captures panes whose session is not busy. A `waiting` session needs action with or without a Plan/Ask prompt (`Session::needs_action`).

## What happens when a session disappears

See the tracking rules in `src/sessions/live.rs` and the summary in `CLAUDE.md`: closing is classified from the pid file, not from the pane.
