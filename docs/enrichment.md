# Enrichment (Plugin Tier)

## Concept

Base detection (`docs/agent-detection.md`) only knows whether a pane is busy and which agent is running. Plugins can promote a pane to a richer tier by writing a JSON file per pane. agent-dash reads these files during the same 200 ms poll and merges the fields into `AgentSession`.

- **Tier 0**: base detection only — agent type, busy/idle from process tree + pane signal.
- **Tier 1**: enrichment file present — overrides scraped status/title and adds `session_id`, `cwd`, `model`, `agent_role`.

## File location and schema

Path: `~/.config/agent-dash/panes/{TMUX_PANE}.json`

See `src/enrichment.rs:13` for the `Enrichment` struct. Fields:

| Field | Type | Notes |
|---|---|---|
| `agent` | string | Required. `"claude"` or `"opencode"`. Must match the detected agent or the file is ignored. |
| `session_id` | string? | Agent's internal session identifier. |
| `status` | `"busy"` or `"idle"` | Overrides scraped busy/idle when present. |
| `cwd` | string? | Working directory. |
| `title` | string? | Overrides pane title display. |
| `model` | string? | Model name (e.g. `"claude-opus-4-5"`). |
| `agent_role` | string? | Arbitrary role label (e.g. `"plan"`, `"exec"`). |
| `updated_at` | string? | ISO timestamp; informational only. |

Plugins write atomically (write to a tempfile, then `rename` into place). On session end / process exit the plugin deletes the file. Because disk is the source of truth, missed events and process restarts are non-issues.

Merge logic: `src/app.rs:179-205`. The agent field is validated first; mismatches are silently skipped.

## Claude plugin

Location: `plugins/claude/`

**How it works**: Claude Code hooks invoke `agent-dash hook-write <event>` with the hook payload on stdin. See `plugins/claude/hooks/hooks.json` for the four events wired up: `SessionStart`, `UserPromptSubmit`, `Stop`, `SessionEnd`.

The `hook-write` subcommand (`src/hook_write.rs`) handles each event:

- `SessionStart` — writes a new enrichment file with `status: idle`, `session_id`, `model`, `cwd`.
- `UserPromptSubmit` — sets `status: busy`; reads the existing file first to preserve `model` and other fields that only arrive at `SessionStart`.
- `Stop` — sets `status: idle`; same read-merge pattern to preserve prior fields.
- `SessionEnd` — deletes the enrichment file.

See `src/hook_write.rs:40` for the `run` function that implements this logic.

## opencode plugin

Location: `plugins/opencode/plugin.ts`

**How it works**: The plugin reads `$TMUX_PANE` once at module load. If the variable is unset it registers empty hooks and becomes inert (`plugins/opencode/plugin.ts:6-9`).

Subscribes to opencode events: `session.created`, `session.updated`, `session.status`, `session.deleted`, and `message.updated` (for model). Maps opencode's `"retry"` status to `"busy"` because the Rust enum only has `busy`/`idle` and a retrying agent is still working (`plugins/opencode/plugin.ts:30-32`).
