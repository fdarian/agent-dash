import { Effect } from "effect"
import { App } from "./ui/app.js"
import { TmuxClient } from "./services/tmux-client.js"

const MainLive = TmuxClient.Default

const program = App.pipe(Effect.provide(MainLive))

Effect.runPromise(program).catch((error) => {
  console.error("Fatal error:", error)
  process.exit(1)
})
