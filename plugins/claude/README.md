# agent-dash Claude Code plugin

An optional Claude Code plugin that enriches agent-dash's view of Claude sessions.
When installed, it hooks into Claude Code's lifecycle events and writes a small JSON
file keyed by `$TMUX_PANE` on session start, each prompt, each response, and on
session end — giving agent-dash a stable `session_id`, accurate busy/idle status,
`cwd`, and model name without polling, HTTP servers, or scraping.

## Installation

The plugin uses Claude Code's plugin system. There are two ways to install it.

### Option A: plugin directory (recommended)

Copy or symlink this folder into `~/.claude/plugins/`:

```sh
mkdir -p ~/.claude/plugins
ln -sf /path/to/agent-dash/plugins/claude ~/.claude/plugins/agent-dash
```

Claude Code picks up plugins from `~/.claude/plugins/*/hooks/hooks.json` automatically.
Restart Claude Code (or open a new session) after linking.

### Option B: merge into settings.json

Add the hook commands directly to your `~/.claude/settings.json` under the `"hooks"` key.
Merge these entries alongside any hooks you already have:

```json
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [{ "type": "command", "command": "agent-dash hook-write session-start" }]
      }
    ],
    "UserPromptSubmit": [
      {
        "hooks": [{ "type": "command", "command": "agent-dash hook-write prompt-submit" }]
      }
    ],
    "Stop": [
      {
        "hooks": [{ "type": "command", "command": "agent-dash hook-write stop" }]
      }
    ],
    "SessionEnd": [
      {
        "hooks": [{ "type": "command", "command": "agent-dash hook-write session-end" }]
      }
    ]
  }
}
```

### Prerequisites

`agent-dash` must be on your `PATH`. Verify with:

```sh
which agent-dash
agent-dash --version
```

## What it writes

Each lifecycle event atomically overwrites:

```
~/.config/agent-dash/panes/{TMUX_PANE}.json
```

Example file for pane `%86`:

```json
{
  "agent": "claude",
  "session_id": "abc123",
  "status": "idle",
  "cwd": "/home/user/my-project",
  "model": "claude-sonnet-4-6",
  "updated_at": "2026-04-26T12:34:56+00:00"
}
```

Field notes:

- `agent` — always `"claude"` for this plugin (required by agent-dash).
- `session_id` — Claude Code's internal session identifier from the hook payload.
- `status` — `"busy"` while Claude is generating a response (UserPromptSubmit),
  `"idle"` after the response completes (Stop) or on session start.
- `cwd` — the working directory Claude Code was invoked from.
- `model` — the model identifier, e.g. `"claude-sonnet-4-6"`. Only present in the
  `SessionStart` payload; subsequent events (Stop, UserPromptSubmit) do not carry it,
  so the hook preserves the last known value from the file rather than dropping it.
- `updated_at` — ISO-8601 timestamp of the last write.

On `SessionEnd`, the file is deleted. The hook has a 5-second timeout on that event
to stay within Claude Code's default 1.5-second SessionEnd budget (overridable via
`CLAUDE_CODE_SESSIONEND_HOOKS_TIMEOUT_MS`).

## What if I don't install it?

agent-dash still works without this plugin. It detects Claude panes by process name
(`claude` in the tmux pane process tree) and infers busy/idle from braille characters
that Claude Code sets in the pane title. The plugin replaces that heuristic with
structured data and adds `session_id`, `cwd`, and `model` that title-scraping cannot
provide.

## Notes on hook payload fields

The following is based on the Claude Code hooks reference at
`docs.anthropic.com/en/docs/claude-code/hooks` (verified April 2026):

- All hook events carry `session_id`, `cwd`, and `hook_event_name` as common fields.
- `SessionStart` additionally carries `model` and `source` (startup/resume/clear/compact).
- `UserPromptSubmit` additionally carries `prompt` (the submitted text).
- `Stop` additionally carries `stop_hook_active` and `last_assistant_message`.
- `SessionEnd` additionally carries `reason` (clear/resume/logout/other).

The `model` field is only present on `SessionStart`. If Claude Code ever adds it
to other events, the hook will pick it up automatically (payload fields take
precedence over the persisted value).

If `$TMUX_PANE` is not set in the environment (Claude Code launched outside tmux),
`hook-write` exits silently without writing anything.
