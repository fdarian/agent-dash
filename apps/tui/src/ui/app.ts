import { Effect, Ref, Schedule, Fiber } from "effect"
import { createCliRenderer, BoxRenderable, KeyEvent } from "@opentui/core"
import type { ClaudeSession } from "../domain/session.js"
import { TmuxClient } from "../services/tmux-client.js"
import { createSessionList } from "./session-list.js"
import { createPanePreview } from "./pane-preview.js"

export const App = Effect.gen(function* () {
  const tmux = yield* TmuxClient

  const renderer = yield* Effect.promise(() => createCliRenderer({
    exitOnCtrlC: true,
    targetFps: 30,
  }))

  const root = new BoxRenderable(renderer, {
    id: "root",
    flexDirection: "row",
    width: "100%",
    height: "100%",
  })
  renderer.root.add(root)

  const sessionList = createSessionList(renderer)
  const panePreview = createPanePreview(renderer)
  root.add(sessionList.box)
  root.add(panePreview.box)

  const sessionsRef = yield* Ref.make<Array<ClaudeSession>>([])
  const selectedIndexRef = yield* Ref.make(0)

  const refreshUI = Effect.gen(function* () {
    const sessions = yield* Ref.get(sessionsRef)
    const selectedIndex = yield* Ref.get(selectedIndexRef)
    sessionList.update(sessions, selectedIndex)

    if (sessions.length > 0 && selectedIndex < sessions.length) {
      const selected = sessions[selectedIndex]
      const content = yield* tmux.capturePaneContent(selected.paneTarget).pipe(
        Effect.catchAll(() => Effect.succeed("(unable to capture pane)")),
      )
      panePreview.update(content)
    } else {
      panePreview.update("")
    }
  })

  const poll = Effect.gen(function* () {
    const sessions = yield* tmux.discoverSessions.pipe(
      Effect.catchAll(() => Effect.succeed([] as Array<ClaudeSession>)),
    )
    yield* Ref.set(sessionsRef, sessions)

    const selectedIndex = yield* Ref.get(selectedIndexRef)
    if (selectedIndex >= sessions.length && sessions.length > 0) {
      yield* Ref.set(selectedIndexRef, sessions.length - 1)
    }

    yield* refreshUI
  })

  const pollingFiber = yield* poll.pipe(
    Effect.repeat(Schedule.fixed("2 seconds")),
    Effect.fork,
  )

  yield* Effect.sync(() => {
    ;(renderer.keyInput as unknown as NodeJS.EventEmitter).on("keypress", (key: KeyEvent) => {
      const handler = Effect.gen(function* () {
        const sessions = yield* Ref.get(sessionsRef)
        const selectedIndex = yield* Ref.get(selectedIndexRef)

        if (key.name === "j" || key.name === "down") {
          if (selectedIndex < sessions.length - 1) {
            yield* Ref.set(selectedIndexRef, selectedIndex + 1)
            yield* refreshUI
          }
        } else if (key.name === "k" || key.name === "up") {
          if (selectedIndex > 0) {
            yield* Ref.set(selectedIndexRef, selectedIndex - 1)
            yield* refreshUI
          }
        } else if (key.name === "q") {
          renderer.destroy()
        }
      })

      Effect.runPromise(handler).catch(() => {})
    })
  })

  renderer.start()

  yield* Effect.async<void>((resume) => {
    renderer.on("destroy", () => {
      resume(Effect.void)
    })
  })

  yield* Fiber.interrupt(pollingFiber)
})
