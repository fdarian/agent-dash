# Design System

The docs site is an engineering-blueprint aesthetic: monospace display, oklch earth palette, warm orange accent, paper-grain noise. The CSS lives in `src/app/global.css` under `.ad-*` class prefixes.

## Palette

All colors are CSS custom properties driven by a single `:root` (light) + `.dark` override.

| Token       | Light                      | Dark                       | Usage |
|-------------|----------------------------|----------------------------|-------|
| `--bg`      | `oklch(97% 0.006 85)`      | `oklch(18% 0.008 155)`     | Page background |
| `--bg-2`    | `oklch(94% 0.006 85)`      | `oklch(15% 0.008 155)`     | Subtle shade, meta-bar fills, tight backgrounds |
| `--panel`   | `oklch(99% 0.004 85)`      | `oklch(21% 0.008 155)`     | Card/panel surfaces (buttons, pager, callouts) |
| `--ink`     | `oklch(20% 0.008 155)`     | `oklch(94% 0.006 85)`      | Primary text |
| `--ink-2`   | `oklch(40% 0.008 155)`     | `oklch(72% 0.006 85)`      | Secondary text |
| `--ink-3`   | `oklch(58% 0.008 155)`     | `oklch(52% 0.006 85)`      | Dim text, labels |
| `--rule`    | `oklch(86% 0.006 85)`      | `oklch(28% 0.008 155)`     | All borders and grid lines |
| `--accent`  | `#D97757`                  | `#D97757`                  | Primary accent, links, CTAs |
| `--ok`      | `oklch(62% 0.14 150)`      | `oklch(72% 0.14 150)`      | Pulsing eyebrow dot |
| `--blue`    | `oklch(58% 0.1 230)`       | `oklch(72% 0.1 230)`       | `.callout.note` dot |
| `--warn`    | `oklch(70% 0.12 75)`       | `oklch(78% 0.12 75)`       | `.callout.warn` dot |

Fumadocs' own `--color-fd-*` tokens are mapped to these in `global.css` so Fumadocs internals (search dialog, etc.) inherit the palette.

## Typography

- **Sans** (`--font-inter-tight`): Inter Tight via `next/font/google` — body, sidebar text
- **Mono** (`--font-jetbrains-mono`): JetBrains Mono — headings, TUI, code, nav, buttons, tables
- Headings are almost all mono; only `h3` uses sans for contrast within a section

## Structural frame

- **Landing** (`.ad-wrap` + `.ad-frame`): 1240px max width, vertical rules on left/right, `.ad-rule` horizontal dividers with 7×7 rotated-square markers at each end
- **Docs** (`.ad-shell`): 1440px, three-column grid — `240px | 1fr | 220px` — with a left and right vertical rule
- **Ruler ticks** (`.ad-ticks-l`, `.ad-ticks-r`): 24px repeating gradient along the landing frame edges, desktop-only

## Paper-grain overlay

`.noise-overlay` on the root layout: fixed, `opacity: 0.035`, background is an inline SVG `<feTurbulence>` — no webp fetch, no external asset.

## Section labels (landing)

`.ad-section-label` renders `01  the core loop  —————————` between sections: numbered accent prefix, uppercase mono label, a `flex:1` rule line.

## Live TUI

`components/landing.tsx` renders a TUI window in two sizes (`sm` hero, `lg` spec). Sessions, ANSI-colored lines, blinking cursor, a foot strip with keybinds. Content animates (setTimeout/setInterval) and is clientside.

## Code blocks (docs)

MDX `pre` is replaced with `Pre` (server) → `CodeShell` (client). Renders as `.ad-code` with a header (language + copy button) and a line-numbered body. No syntax highlighting — raw text only. If you want Shiki-tokenized output back, wire it into `CodeShell` and be careful about server/client children divergence.

## Callouts

MDX `<Callout kind="tip|note|warn" title="...">` renders `.ad-doc-callout` — a bordered card with a colored dot + title row. `kind` controls the dot color.

## Numbered H2

Docs section numbers are CSS-generated via `counter-reset` on `.ad-main` and `counter(ad-section, decimal-leading-zero)` on `h2::before`. MDX authors just write `## Heading` — don't add a number span.

## Buttons

- `.ad-btn` — monospace, pill-rounded, 1px border, subtle hover
- `.ad-btn.primary` — accent fill, inset highlight, stronger shadow
- `.ad-btn .key` — inline small-caps keybind pill inside the button

## Keybind board (landing)

`.ad-keygrid` — 4-col grid of `.ad-kb` cells. Each cell has a `.k` keycap (mono, bottom-weighted border) + description. `.k.accent` fills the keycap with the brand orange.

## TOC meta

Right rail footer block (`.ad-toc .meta`) with Updated / Version / Read time / "edit on github" link. Values passed from page component; `editUrl` points at the MDX file on GitHub.
