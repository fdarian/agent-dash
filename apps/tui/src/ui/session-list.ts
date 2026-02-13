import {
  BoxRenderable,
  TextRenderable,
  type CliRenderer,
} from "@opentui/core"
import type { ClaudeSession } from "../domain/session.js"

export function createSessionList(renderer: CliRenderer) {
  const box = new BoxRenderable(renderer, {
    id: "session-list",
    width: 40,
    flexDirection: "column",
    border: true,
    title: "Sessions",
    paddingX: 1,
  })

  let childIds: Array<string> = []

  function update(sessions: Array<ClaudeSession>, selectedIndex: number) {
    for (const id of childIds) {
      box.remove(id)
    }
    childIds = []

    for (let i = 0; i < sessions.length; i++) {
      const session = sessions[i]
      const isSelected = i === selectedIndex
      const icon = session.status === "active" ? "●" : "○"
      const id = `session-item-${i}`

      const text = new TextRenderable(renderer, {
        id,
        content: `${icon} ${session.title || session.sessionName}`,
        fg: isSelected ? "#FFFFFF" : "#AAAAAA",
        bg: isSelected ? "#444444" : undefined,
      })

      box.add(text)
      childIds.push(id)
    }

    if (sessions.length === 0) {
      const emptyId = "session-empty"
      const text = new TextRenderable(renderer, {
        id: emptyId,
        content: "No Claude sessions found",
        fg: "#666666",
      })
      box.add(text)
      childIds.push(emptyId)
    }
  }

  return { box, update }
}
