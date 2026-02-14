import {
  BoxRenderable,
  TextRenderable,
  ScrollBoxRenderable,
  type CliRenderer,
} from "@opentui/core"
import { parseAnsiToStyledText } from "./ansi-parser.js"
import { PRIMARY_COLOR, UNFOCUSED_COLOR } from "./constants.js"

export function createPanePreview(renderer: CliRenderer) {
  const box = new BoxRenderable(renderer, {
    id: "pane-preview",
    flexGrow: 1,
    flexDirection: "column",
    border: true,
    title: "[0] Preview",
  })

  const scrollBox = new ScrollBoxRenderable(renderer, {
    id: "pane-preview-scroll",
    flexGrow: 1,
    flexDirection: "column",
    stickyScroll: true,
    scrollY: true,
  })

  box.add(scrollBox)

  let lineIds: Array<string> = []

  function setFocused(focused: boolean) {
    box.borderColor = focused ? PRIMARY_COLOR : UNFOCUSED_COLOR
  }

  function update(content: string) {
    for (const id of lineIds) {
      scrollBox.remove(id)
    }
    lineIds = []

    const lines = content.split("\n")
    for (let i = 0; i < lines.length; i++) {
      const id = `pane-line-${i}`
      const styledContent = parseAnsiToStyledText(lines[i])
      const text = new TextRenderable(renderer, {
        id,
        content: styledContent,
      })
      scrollBox.add(text)
      lineIds.push(id)
    }
  }

  function scrollBy(amount: number) {
    scrollBox.scrollBy(amount, "step")
  }

  setFocused(false)

  return { box, update, setFocused, scrollBy }
}
