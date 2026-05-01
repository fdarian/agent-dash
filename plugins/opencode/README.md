# agent-dash opencode plugin

An optional opencode plugin that enriches agent-dash's view of opencode sessions.
When installed, it writes a small JSON file keyed by `$TMUX_PANE` on every session
event — giving agent-dash a stable `session_id`, accurate busy/idle status, `cwd`,
model name, and session title without any polling, HTTP servers, or scraping.

## Installation

Symlink (or copy) the plugin into the opencode global plugin directory:

```sh
mkdir -p ~/.config/opencode/plugins
ln -sf /path/to/agent-dash/plugins/opencode/plugin.ts ~/.config/opencode/plugins/agent-dash.ts
```

Then restart opencode (or start a new opencode instance — plugins are loaded at startup).

For project-level installation instead of global, use `.opencode/plugins/agent-dash.ts`
inside your project directory.

## What it writes

Each time a session event fires, the plugin atomically overwrites:

```
~/.config/agent-dash/panes/{TMUX_PANE}.json
```

Example file for pane `%86`:

```json
{
  "agent": "opencode",
  "session_id": "ses_abc123",
  "status": "busy",
  "cwd": "/home/user/my-project",
  "title": "Refactor the parser",
  "model": "anthropic/claude-opus-4-5",
  "updated_at": "2026-04-26T12:34:56.789Z"
}
```

Field notes:

- `agent` — always `"opencode"` for this plugin (required by agent-dash).
- `session_id` — opencode's internal session identifier.
- `status` — `"busy"` while the agent is processing (including retries), `"idle"` when done.
- `cwd` — the directory opencode was pointed at when the session started.
- `title` — the session title set by opencode (auto-generated or user-provided).
- `model` — formatted as `"providerID/modelID"`, e.g. `"anthropic/claude-opus-4-5"`. Populated once the first assistant message arrives.
- `updated_at` — ISO-8601 timestamp of the last write.

The file is deleted automatically when the session ends or the opencode process exits.

## What if I don't install it?

agent-dash still works without this plugin. It detects opencode panes by process name
(`opencode` in the tmux pane process tree) and infers busy/idle by scanning pane
content for the `"esc interrupt"` status-bar substring that opencode displays while
busy. The plugin just replaces that heuristic with structured data, and adds
`session_id`, `cwd`, `model`, and `title` that content-scraping cannot provide.

## API decisions and assumptions

- **Plugin directory**: official docs list `~/.config/opencode/plugins/` (plural).
  The task spec mentioned the singular `plugin/` form; the plural form from the
  live docs was used here as the canonical path.
- **`session.status` retry variant**: opencode emits `{ type: "retry", ... }` during
  backoff. This is mapped to `"busy"` because the agent is still active. Writing
  the literal `"retry"` string would cause agent-dash's parser to silently discard
  the enrichment file.
- **`model` field**: not available on session creation events; populated on the first
  `message.updated` event where `role === "assistant"` carries `modelID`/`providerID`.
- **`agent_role` field**: opencode has no direct equivalent, so this field is omitted.
  agent-dash treats missing fields as "no opinion".
- **Concurrent writes**: events can fire in rapid succession (e.g. `session.status`
  followed immediately by `session.updated`). Writes are serialized via a promise
  chain (`writeQueue`) so the last write always wins without partial-file risk.
- **Process exit cleanup**: `process.on('exit')` callbacks run synchronously with no
  async support, so `fs.unlinkSync` is used there. SIGINT/SIGTERM handlers use the
  async `fs.unlink` and call `process.exit(0)` when done.
