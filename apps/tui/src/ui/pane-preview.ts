import { TextRenderable, ScrollBoxRenderable, type CliRenderer } from "@opentui/core"
import { PRIMARY_COLOR, UNFOCUSED_COLOR } from "./constants.js"
import { parseAnsiToStyledText } from "./ansi-parser.js"

export function createPanePreview(renderer: CliRenderer) {
  const scrollBox = new ScrollBoxRenderable(renderer, {
    id: "pane-preview",
    flexGrow: 1,
    border: true,
    title: "[0] Preview",
    stickyScroll: true,
    scrollY: true,
  })

  const textContent = new TextRenderable(renderer, {
    id: "pane-preview-content",
    content: "",
  })
  scrollBox.add(textContent)

  function setFocused(focused: boolean) {
    scrollBox.borderColor = focused ? PRIMARY_COLOR : UNFOCUSED_COLOR
  }

  function update(content: string) {
    textContent.content = parseAnsiToStyledText(content)
  }

  function scrollBy(amount: number) {
    scrollBox.scrollBy(amount, "step")
  }

  setFocused(false)

  return { box: scrollBox, update, setFocused, scrollBy }
}
