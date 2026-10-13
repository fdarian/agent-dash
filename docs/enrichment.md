# Enrichment (Plugin Tier)

## Concept

Base detection (`docs/agent-detection.md`) only knows whether a pane is busy and which agent is running. The opencode plugin promotes a pane to a richer tier by writing a JSON file per pane. agent-dash reads these files during the same 2 s poll and merges the fields into `AgentSession`.

- **Tier 0**: base detection only — agent type, busy/idle from process tree + pane signal.
- **Tier 1**: enrichment file present — overrides scraped status/title and adds `session_id`, `cwd`, `model`, `agent_role`.

## File location and schema

Path: `~/.config/agent-dash/panes/{TMUX_PANE}.json`

See `src/enrichment.rs` for the `Enrichment` struct. Fields:

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

Merge logic: the poll loop in `src/app.rs`. The agent field is validated first; mismatches are silently skipped.

## Claude

Claude needs no plugin. Claude Code writes its own per-process file, `~/.claude/sessions/<claude_pid>.json`, which `src/claude_sessions.rs` reads (status, `sessionId`, `cwd`). It is applied in `src/app.rs` right after the enrichment merge, so it also wins over any stale `panes/*.json` file for the same pane. See `docs/agent-detection.md` for how the PID is obtained.

Claude's `last_activity` (the time shown in the list and used for flat-view buckets) comes from the session transcript, not tmux `window_activity`, which moves on any window output. `src/claude_transcript.rs` finds `~/.claude/projects/<cwd with non-alphanumerics replaced by "-">/<session_id>.jsonl` (falling back to scanning all project dirs), reads backwards from the end for the last `user`/`assistant` line, and uses its `timestamp`. Results are cached per session id and re-parsed only when the file's length or mtime changes. If no transcript is readable, the tmux value is kept.

## opencode plugin

Location: `plugins/opencode/plugin.ts`

**How it works**: The plugin reads `$TMUX_PANE` once at module load. If the variable is unset it registers empty hooks and becomes inert (`plugins/opencode/plugin.ts`).

Subscribes to opencode events: `session.created`, `session.updated`, `session.status`, `session.deleted`, and `message.updated` (for model). Maps opencode's `"retry"` status to `"busy"` because the Rust enum only has `busy`/`idle` and a retrying agent is still working (`plugins/opencode/plugin.ts`).
