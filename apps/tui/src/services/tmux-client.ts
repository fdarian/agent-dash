import { Effect, Context, Layer, Data } from "effect"
import { type ClaudeSession, parseSessionStatus } from "../domain/session.js"

export class TmuxError extends Data.TaggedError("TmuxError")<{
  message: string
  command: string
}> {}

interface TmuxClientImpl {
  discoverSessions: Effect.Effect<Array<ClaudeSession>, TmuxError>
  capturePaneContent: (paneTarget: string) => Effect.Effect<string, TmuxError>
}

export class TmuxClient extends Context.Tag("TmuxClient")<
  TmuxClient,
  TmuxClientImpl
>() {
  static Live = Layer.succeed(
    TmuxClient,
    TmuxClient.of({
      discoverSessions: Effect.gen(function* () {
        const format = [
          "#{pane_id}",
          "#{pane_pid}",
          "#{pane_title}",
          "#{session_name}:#{window_index}.#{pane_index}",
        ].join("\t")

        const output = yield* runCommand("tmux", [
          "list-panes",
          "-a",
          "-F",
          format,
        ])

        const lines = output.trim().split("\n").filter(Boolean)
        const sessions: Array<ClaudeSession> = []

        for (const line of lines) {
          const parts = line.split("\t")
          if (parts.length < 4) continue

          const paneId = parts[0]
          const panePid = parts[1]
          const paneTitle = parts[2]
          const paneTarget = parts[3]
          const sessionName = paneTarget.split(":")[0]

          const isClaude = yield* checkForClaudeProcess(panePid)
          if (!isClaude) continue

          sessions.push({
            paneId,
            paneTarget,
            title: paneTitle,
            sessionName,
            status: parseSessionStatus(paneTitle),
          })
        }

        return sessions
      }),

      capturePaneContent: (paneTarget) =>
        runCommand("tmux", ["capture-pane", "-e", "-t", paneTarget, "-p"]),
    }),
  )

  static Default = TmuxClient.Live
}

function runCommand(
  cmd: string,
  args: Array<string>,
): Effect.Effect<string, TmuxError> {
  return Effect.tryPromise({
    try: async () => {
      const proc = Bun.spawn([cmd, ...args], {
        stdout: "pipe",
        stderr: "pipe",
      })
      const exitCode = await proc.exited
      const stdout = await new Response(proc.stdout).text()
      const stderr = await new Response(proc.stderr).text()
      if (exitCode !== 0) {
        throw new Error(stderr || `Process exited with code ${exitCode}`)
      }
      return stdout
    },
    catch: (error) =>
      new TmuxError({
        message: error instanceof Error ? error.message : String(error),
        command: `${cmd} ${args.join(" ")}`,
      }),
  })
}

function checkForClaudeProcess(
  parentPid: string,
): Effect.Effect<boolean, TmuxError> {
  return Effect.gen(function* () {
    const pgrepOutput = yield* runCommand("pgrep", ["-P", parentPid]).pipe(
      Effect.catchAll(() => Effect.succeed("")),
    )

    const childPids = pgrepOutput.trim().split("\n").filter(Boolean)

    for (const childPid of childPids) {
      const comm = yield* runCommand("ps", ["-o", "comm=", "-p", childPid]).pipe(
        Effect.catchAll(() => Effect.succeed("")),
      )
      if (comm.trim().endsWith("claude")) return true

      const nested = yield* checkForClaudeProcess(childPid)
      if (nested) return true
    }

    return false
  })
}
