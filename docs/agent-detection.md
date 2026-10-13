# Agent Detection

## Process detection

Every 2 s the polling task in `src/app.rs` calls `tmux list-panes` and, for each pane with a live process, calls `detect_agent` (`src/tmux.rs`).

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

If no matching process is found in the tree, `detect_agent` returns `None`. Otherwise it returns a `DetectedAgent` carrying the agent and the PID of the matched process.

The result is stored as `AgentSession.agent` (`src/session.rs`, `Agent` enum) and `AgentSession.agent_pid`. The PID is `#[serde(skip)]` because a cached PID means nothing after a restart.

## Busy/idle status

Detection branches on the agent variant. See `src/session.rs:parse_session_status`.

**Claude**: Claude Code writes `~/.claude/sessions/<claude_pid>.json` for each live process; the poll loop reads it using `AgentSession.agent_pid` (`src/claude_sessions.rs`). Its `status` (`busy` / `idle`) overrides the scraped status, and its `sessionId` and `cwd` populate the session. An unrecognized status value leaves the scraped status untouched. The `tmux` field in that file is often missing, so the PID is the join key.

When the file is absent (older Claude versions), the fallback is `parse_session_status`: Claude writes braille characters (U+2800-U+28FF) into the tmux pane title while busy, and only the first character of the title is inspected.

**opencode**: The pane title is always the static string `"OpenCode"` and gives no signal. Instead, `parse_session_status` scans the last ~5 visible lines of pane content for the substring `"esc interrupt"`, which appears in the opencode status bar only while an agent is running. This reuses the existing `tmux capture-pane` output from the same 2 s tick — no extra capture.

## Prompt state (Plan / Ask)

`src/session.rs:detect_prompt_state` parses the last non-empty line of pane content for Plan/Ask state. This is Claude-only: if `agent == Agent::Opencode` the function returns `None`.

## Adding a new agent

1. Add a variant to `enum Agent` in `src/session.rs`.
2. Add a leaf-name match in `src/tmux.rs:detect_agent`.
3. Add a branch in `src/session.rs:parse_session_status` for the new agent's busy signal.
4. Add a branch in `src/session.rs:detect_prompt_state` if the agent exposes prompt state, otherwise guard with an early return like the opencode case.
5. Optionally ship a plugin that writes enrichment files — see `docs/enrichment.md`.
