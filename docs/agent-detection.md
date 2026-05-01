# Agent Detection

## Process detection

Every 200 ms the polling task in `src/app.rs` calls `tmux list-panes` and, for each pane with a live process, calls `detect_agent` (`src/tmux.rs:347`).

`detect_agent` walks the pane's process tree recursively via `pgrep` and matches the leaf process name:

- `claude` -> `Agent::Claude`
- `opencode` -> `Agent::Opencode`
- anything else -> keep walking children; return `None` if tree is exhausted

The result is stored as `AgentSession.agent` (`src/session.rs`, `Agent` enum at line 5).

## Busy/idle status

Detection branches on the agent variant. See `src/session.rs:parse_session_status` (line 48).

**Claude**: The Claude CLI writes braille characters (U+2800-U+28FF) into the tmux pane title while busy. `parse_session_status` inspects only the first character of the pane title — no content scan needed.

**opencode**: The pane title is always the static string `"OpenCode"` and gives no signal. Instead, `parse_session_status` scans the last ~5 visible lines of pane content for the substring `"esc interrupt"`, which appears in the opencode status bar only while an agent is running. This reuses the existing `tmux capture-pane` output from the same 200 ms tick — no extra capture.

## Prompt state (Plan / Ask)

`src/session.rs:detect_prompt_state` (line 86) parses the last non-empty line of pane content for Plan/Ask state. This is Claude-only: if `agent == Agent::Opencode` the function returns `None`.

## Adding a new agent

1. Add a variant to `enum Agent` in `src/session.rs`.
2. Add a leaf-name match in `src/tmux.rs:detect_agent` (line 347).
3. Add a branch in `src/session.rs:parse_session_status` for the new agent's busy signal.
4. Add a branch in `src/session.rs:detect_prompt_state` if the agent exposes prompt state, otherwise guard with an early return like the opencode case.
5. Optionally ship a plugin that writes enrichment files — see `docs/enrichment.md`.
