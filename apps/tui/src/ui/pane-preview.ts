import {
  BoxRenderable,
  TextRenderable,
  ScrollBoxRenderable,
  type CliRenderer,
} from "@opentui/core"

export function createPanePreview(renderer: CliRenderer) {
  const box = new BoxRenderable(renderer, {
    id: "pane-preview",
    flexGrow: 1,
    flexDirection: "column",
    border: true,
    title: "Preview",
  })

  const scrollBox = new ScrollBoxRenderable(renderer, {
    id: "pane-preview-scroll",
    flexGrow: 1,
    flexDirection: "column",
    stickyScroll: true,
  })

  box.add(scrollBox)

  let lineIds: Array<string> = []

  function update(content: string) {
    for (const id of lineIds) {
      scrollBox.remove(id)
    }
    lineIds = []

    const lines = content.split("\n")
    for (let i = 0; i < lines.length; i++) {
      const id = `pane-line-${i}`
      const text = new TextRenderable(renderer, {
        id,
        content: lines[i],
      })
      scrollBox.add(text)
      lineIds.push(id)
    }
  }

  return { box, update }
}
