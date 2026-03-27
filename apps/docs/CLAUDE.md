# CLAUDE.md — apps/docs

Fumadocs documentation site for Agent Dash.

## Stack

- **Framework**: Next.js 16 (app router)
- **Docs framework**: Fumadocs (fumadocs-ui, fumadocs-core, fumadocs-mdx)
- **Styling**: Tailwind CSS 4 + Fumadocs preset
- **Content**: MDX files in `content/docs/`

## Design

See [notes/design-system.md](./notes/design-system.md) for the full design system (colors, grid, typography, components).

## Structure

- `src/app/page.tsx` — Landing page (custom, not using Fumadocs layout)
- `src/app/docs/` — Docs section (Fumadocs DocsLayout)
- `src/app/global.css` — Theme overrides, grid CSS, fancy button, animations
- `src/components/theme-toggle.tsx` — Dark/light toggle for landing nav
- `src/lib/source.ts` — Fumadocs source loader
- `content/docs/` — MDX documentation pages
- `source.config.ts` — Fumadocs MDX collection config
- `public/noise.webp` — Noise texture for paper-like background

## Non-obvious details

- The landing page grid is structural (CSS borders + positioned SVG diamonds), not a repeating background pattern
- Fumadocs dark theme colors are overridden in `global.css` via `.dark {}` selector to achieve the navy-slate Zed-like palette
- Sidebar darkness is forced with `!important` on `#nd-sidebar` because Fumadocs applies its own bg
- The noise overlay is in the root layout so it covers both landing and docs pages
- The `--color-sidebar` CSS variable switches between light/dark values and is used both for the sidebar `background-color` and the grid diamond `fill`
