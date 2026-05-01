import fs from "fs/promises"
import fsSync from "fs"
import path from "path"
import os from "os"

const paneId = process.env["TMUX_PANE"]

if (!paneId) {
  console.warn("[agent-dash] TMUX_PANE is not set — plugin is a no-op")
}

function enrichmentDir(): string {
  return path.join(os.homedir(), ".config", "agent-dash", "panes")
}

function enrichmentPath(): string {
  return path.join(enrichmentDir(), `${paneId}.json`)
}

function tmpPath(): string {
  return path.join(enrichmentDir(), `${paneId}.json.tmp`)
}

async function ensureDir(): Promise<void> {
  await fs.mkdir(enrichmentDir(), { recursive: true })
}

// Maps opencode SessionStatus.type to agent-dash enrichment status.
// "retry" means the agent is still working, so we treat it as busy.
function mapStatus(statusType: string): "busy" | "idle" {
  if (statusType === "idle") return "idle"
  return "busy"
}

async function writeEnrichment(data: Record<string, string | undefined>): Promise<void> {
  await ensureDir()
  const payload: Record<string, string | undefined> = {
    agent: "opencode",
    updated_at: new Date().toISOString(),
  }
  for (const key of Object.keys(data)) {
    payload[key] = data[key]
  }
  const json = JSON.stringify(payload, null, 2)
  const tmp = tmpPath()
  const target = enrichmentPath()
  await fs.writeFile(tmp, json, "utf-8")
  await fs.rename(tmp, target)
}

async function removeEnrichment(): Promise<void> {
  try {
    await fs.unlink(enrichmentPath())
  } catch (err: unknown) {
    if ((err as NodeJS.ErrnoException).code !== "ENOENT") {
      console.warn("[agent-dash] Failed to remove enrichment file:", err)
    }
  }
}

function removeEnrichmentSync(): void {
  try {
    fsSync.unlinkSync(enrichmentPath())
  } catch (err: unknown) {
    if ((err as NodeJS.ErrnoException).code !== "ENOENT") {
      console.warn("[agent-dash] Failed to remove enrichment file (sync):", err)
    }
  }
}

export const AgentDashPlugin = async (ctx: {
  client: unknown
  project: { id: string; worktree: string }
  directory: string
  worktree: string
  serverUrl: URL
}) => {
  if (!paneId) {
    // TMUX_PANE was unset at module load — return empty hooks, plugin is inert.
    return {}
  }

  // Mutable state accumulated from events across the session lifetime.
  let currentSessionId: string | undefined
  let currentStatus: "busy" | "idle" | undefined
  let currentCwd: string | undefined
  let currentTitle: string | undefined
  let currentModel: string | undefined

  // Chain writes to avoid interleaving when events fire in rapid succession.
  let writeQueue: Promise<void> = Promise.resolve()

  function enqueue(task: () => Promise<void>): void {
    writeQueue = writeQueue.then(task).catch((err: unknown) => {
      console.warn("[agent-dash] Error writing enrichment file:", err)
    })
  }

  function buildPayload(): Record<string, string | undefined> {
    const payload: Record<string, string | undefined> = {}
    if (currentSessionId !== undefined) payload["session_id"] = currentSessionId
    if (currentStatus !== undefined) payload["status"] = currentStatus
    if (currentCwd !== undefined) payload["cwd"] = currentCwd
    if (currentTitle !== undefined) payload["title"] = currentTitle
    if (currentModel !== undefined) payload["model"] = currentModel
    return payload
  }

  function handleSessionInfo(info: {
    id: string
    directory: string
    title: string
  }): void {
    currentSessionId = info.id
    currentCwd = info.directory
    currentTitle = info.title
  }

  // Register cleanup on process exit (sync-only context, must use sync API).
  process.on("exit", () => {
    removeEnrichmentSync()
  })

  // Register cleanup on signals — these support async.
  const handleSignal = async (): Promise<void> => {
    await removeEnrichment()
    process.exit(0)
  }

  process.on("SIGINT", handleSignal)
  process.on("SIGTERM", handleSignal)

  return {
    event: async (input: { event: { type: string; properties: Record<string, unknown> } }): Promise<void> => {
      const event = input.event

      try {
        if (event.type === "session.created") {
          const info = event.properties["info"] as {
            id: string
            directory: string
            title: string
          }
          handleSessionInfo(info)
          enqueue(() => writeEnrichment(buildPayload()))
          return
        }

        if (event.type === "session.updated") {
          const info = event.properties["info"] as {
            id: string
            directory: string
            title: string
          }
          handleSessionInfo(info)
          enqueue(() => writeEnrichment(buildPayload()))
          return
        }

        if (event.type === "session.status") {
          const props = event.properties as {
            sessionID: string
            status: { type: string }
          }
          currentSessionId = props.sessionID
          currentStatus = mapStatus(props.status.type)
          enqueue(() => writeEnrichment(buildPayload()))
          return
        }

        if (event.type === "session.idle") {
          const props = event.properties as { sessionID: string }
          currentSessionId = props.sessionID
          currentStatus = "idle"
          enqueue(() => writeEnrichment(buildPayload()))
          return
        }

        if (event.type === "session.deleted") {
          enqueue(() => removeEnrichment())
          currentSessionId = undefined
          currentStatus = undefined
          currentCwd = undefined
          currentTitle = undefined
          currentModel = undefined
          return
        }

        if (event.type === "message.updated") {
          const info = event.properties["info"] as {
            role?: string
            modelID?: string
            providerID?: string
          }
          // Only assistant messages carry model info.
          if (info.role === "assistant" && info.modelID) {
            const providerPrefix = info.providerID ? `${info.providerID}/` : ""
            currentModel = `${providerPrefix}${info.modelID}`
            enqueue(() => writeEnrichment(buildPayload()))
          }
          return
        }
      } catch (err: unknown) {
        // Never let plugin errors escape — opencode must keep working.
        console.warn("[agent-dash] Unhandled error in event handler:", err)
      }
    },
  }
}

export default AgentDashPlugin
