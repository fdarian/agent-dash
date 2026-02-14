import {
  StyledText,
  type Color,
  RGBA,
  TextAttributes,
  type TextChunk,
} from "@opentui/core"

const ANSI_256_COLORS: string[] = (() => {
  const colors: string[] = []

  colors[0] = "#000000"
  colors[1] = "#AA0000"
  colors[2] = "#00AA00"
  colors[3] = "#AA5500"
  colors[4] = "#0000AA"
  colors[5] = "#AA00AA"
  colors[6] = "#00AAAA"
  colors[7] = "#AAAAAA"
  colors[8] = "#555555"
  colors[9] = "#FF5555"
  colors[10] = "#55FF55"
  colors[11] = "#FFFF55"
  colors[12] = "#5555FF"
  colors[13] = "#FF55FF"
  colors[14] = "#55FFFF"
  colors[15] = "#FFFFFF"

  for (let i = 0; i < 216; i++) {
    const r = Math.floor(i / 36) * 51
    const g = (Math.floor(i / 6) % 6) * 51
    const b = (i % 6) * 51
    colors[16 + i] = `#${toHex(r)}${toHex(g)}${toHex(b)}`
  }

  for (let i = 0; i < 24; i++) {
    const gray = 8 + i * 10
    colors[232 + i] = `#${toHex(gray)}${toHex(gray)}${toHex(gray)}`
  }

  return colors
})()

function toHex(value: number): string {
  return value.toString(16).padStart(2, "0")
}

interface StyleState {
  fg: Color | undefined
  bg: Color | undefined
  bold: boolean
  dim: boolean
  italic: boolean
  underline: boolean
  strikethrough: boolean
  reverse: boolean
}

function createDefaultState(): StyleState {
  return {
    fg: undefined,
    bg: undefined,
    bold: false,
    dim: false,
    italic: false,
    underline: false,
    strikethrough: false,
    reverse: false,
  }
}

function stateToAttributes(state: StyleState): number {
  let attrs = TextAttributes.NONE
  if (state.bold) attrs |= TextAttributes.BOLD
  if (state.dim) attrs |= TextAttributes.DIM
  if (state.italic) attrs |= TextAttributes.ITALIC
  if (state.underline) attrs |= TextAttributes.UNDERLINE
  if (state.strikethrough) attrs |= TextAttributes.STRIKETHROUGH
  if (state.reverse) attrs |= TextAttributes.INVERSE
  return attrs
}

function parseAnsiCode(code: string, state: StyleState): void {
  const parts = code.split(";").map((p) => parseInt(p, 10))
  let i = 0

  while (i < parts.length) {
    const n = parts[i]

    if (n === 0) {
      state.fg = undefined
      state.bg = undefined
      state.bold = false
      state.dim = false
      state.italic = false
      state.underline = false
      state.strikethrough = false
      state.reverse = false
    } else if (n === 1) {
      state.bold = true
    } else if (n === 2) {
      state.dim = true
    } else if (n === 3) {
      state.italic = true
    } else if (n === 4) {
      state.underline = true
    } else if (n === 7) {
      state.reverse = true
    } else if (n === 9) {
      state.strikethrough = true
    } else if (n === 22) {
      state.bold = false
      state.dim = false
    } else if (n === 23) {
      state.italic = false
    } else if (n === 24) {
      state.underline = false
    } else if (n === 27) {
      state.reverse = false
    } else if (n === 29) {
      state.strikethrough = false
    } else if (n >= 30 && n <= 37) {
      state.fg = ANSI_256_COLORS[n - 30]
    } else if (n === 38) {
      if (i + 1 < parts.length && parts[i + 1] === 5) {
        if (i + 2 < parts.length) {
          const colorIndex = parts[i + 2]
          const fgColor = ANSI_256_COLORS[colorIndex]
          if (fgColor !== undefined) {
            state.fg = fgColor
          }
          i += 2
        }
      } else if (i + 1 < parts.length && parts[i + 1] === 2) {
        if (i + 4 < parts.length) {
          const r = parts[i + 2]
          const g = parts[i + 3]
          const b = parts[i + 4]
          state.fg = `#${toHex(r)}${toHex(g)}${toHex(b)}`
          i += 4
        }
      }
    } else if (n === 39) {
      state.fg = undefined
    } else if (n >= 40 && n <= 47) {
      state.bg = ANSI_256_COLORS[n - 40]
    } else if (n === 48) {
      if (i + 1 < parts.length && parts[i + 1] === 5) {
        if (i + 2 < parts.length) {
          const colorIndex = parts[i + 2]
          const bgColor = ANSI_256_COLORS[colorIndex]
          if (bgColor !== undefined) {
            state.bg = bgColor
          }
          i += 2
        }
      } else if (i + 1 < parts.length && parts[i + 1] === 2) {
        if (i + 4 < parts.length) {
          const r = parts[i + 2]
          const g = parts[i + 3]
          const b = parts[i + 4]
          state.bg = `#${toHex(r)}${toHex(g)}${toHex(b)}`
          i += 4
        }
      }
    } else if (n === 49) {
      state.bg = undefined
    } else if (n >= 90 && n <= 97) {
      state.fg = ANSI_256_COLORS[n - 90 + 8]
    } else if (n >= 100 && n <= 107) {
      state.bg = ANSI_256_COLORS[n - 100 + 8]
    }

    i++
  }
}

function createChunk(text: string, state: StyleState): TextChunk {
  const chunk: TextChunk = {
    __isChunk: true,
    text,
  }

  if (state.fg) {
    chunk.fg = typeof state.fg === "string" ? RGBA.fromHex(state.fg) : state.fg
  }

  if (state.bg) {
    chunk.bg = typeof state.bg === "string" ? RGBA.fromHex(state.bg) : state.bg
  }

  const attrs = stateToAttributes(state)
  if (attrs !== TextAttributes.NONE) {
    chunk.attributes = attrs
  }

  return chunk
}

export function parseAnsiToStyledText(raw: string): StyledText {
  const chunks: TextChunk[] = []
  const state = createDefaultState()
  const ansiRegex = /\x1b\[([0-9;]*)m/g

  let lastIndex = 0
  let match: RegExpExecArray | null

  while ((match = ansiRegex.exec(raw)) !== null) {
    if (match.index > lastIndex) {
      const text = raw.slice(lastIndex, match.index)
      if (text.length > 0) {
        chunks.push(createChunk(text, state))
      }
    }

    parseAnsiCode(match[1], state)
    lastIndex = ansiRegex.lastIndex
  }

  if (lastIndex < raw.length) {
    const text = raw.slice(lastIndex)
    if (text.length > 0) {
      chunks.push(createChunk(text, state))
    }
  }

  return new StyledText(chunks)
}
