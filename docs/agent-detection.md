# Agent Detection

## Process detection

Every 200 ms the polling task in `src/app.rs` calls `tmux list-panes` and, for each pane with a live process, calls `detect_agent` (`src/tmux.rs:347`).

`detect_agent` walks the pane's process tree recursively via `pgrep` and inspects each process's executable name (`comm`) and command-line arguments (`args`). A process is considered an agent session only when:

1. The executable basename is exactly `claude` or `opencode` (case-sensitive, after stripping any leading `-` login-shell prefix).
2. The command has no positional subcommand after the executable — only flags (arguments starting with `-`) are allowed.

Examples of accepted processes:
- `claude`
- `claude --model opus --agent guide`
- `opencode`
- `opencode --model anthropic/claude-sonnet-4-5`

Examples of rejected processes:
- `oagent serve` (wrong executable name)
- `opencode acp` (positional subcommand)
- `claude mcp serve` (positional subcommand)
- `claude doctor` (positional subcommand)

If no matching process is found in the tree, `detect_agent` returns `None`.

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
