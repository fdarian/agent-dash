# CLAUDE.md — apps/docs

Fumadocs documentation site for Agent Dash.

## Stack

- **Framework**: Next.js 16 (app router)
- **Docs framework**: Fumadocs MDX loader + source API (UI shell is custom, not `DocsLayout`)
- **Styling**: Tailwind CSS 4 + custom CSS in `global.css`
- **Content**: MDX files in `content/docs/`

## Design

See [notes/design-system.md](./notes/design-system.md) for the design system (colors, typography, components).

## Structure

- `src/app/page.tsx` — Landing page entry (delegates to `components/landing.tsx`)
- `src/components/landing.tsx` — Landing page UI (Hero, SpecSheet, Keys, Signals, Install)
- `src/app/docs/layout.tsx` — Custom docs shell: `DocsNav` + children + footer
- `src/app/docs/[[...slug]]/page.tsx` — Renders MDX inside the custom sidebar/article/TOC grid
- `src/components/docs/` — Custom docs chrome: `docs-nav`, `sidebar`, `toc`, `callout`, `code-block`, `code-shell`, `nav-data`
- `src/components/mdx.tsx` — MDX component overrides (custom `pre`, `a`, inline `code`, `Callout`)
- `src/app/global.css` — All component styling (`.ad-*` classes, theme palette, noise overlay)
- `src/lib/source.ts` — Fumadocs source loader
- `content/docs/` — MDX documentation pages
- `source.config.ts` — Fumadocs MDX collection config
- `public/noise.webp` — Noise texture (currently replaced by inline SVG turbulence in CSS)

## Non-obvious details

- The landing page uses a fixed 1240px `.ad-wrap` frame; the docs uses a wider 1440px `.ad-shell` grid
- Numbered h2 badges in docs are **CSS-generated** via `counter-reset: ad-section` on `.ad-main` and `counter(ad-section, decimal-leading-zero)` on `h2::before`. Do not add a `<span className="num">` in MDX — it will double up.
- The TOC extracts text from `page.data.toc` titles via a recursive `toText` walker because Fumadocs returns `ReactNode`, not strings
- The custom `Pre` component (MDX `pre` override) is a **server component** that delegates to a client-side `CodeShell` for the copy button. Splitting avoids a hydration mismatch caused by Shiki-tokenized children being re-parsed on the client.
- Do not use a non-standard ```keybind code fence — Shiki doesn't know it and throws at build time. Use ```text or ```bash.
- Paper-grain noise is a fixed overlay rendered by `.noise-overlay` in the root layout. The texture is an inline SVG `<feTurbulence>` (no webp fetch).
- Theme: `:root` = light (default), `.dark` = overrides. `RootProvider` sets `defaultTheme: 'light'` with `enableSystem: false` so it doesn't follow the OS preference.
- The sidebar uses `pathname === it.href` for active-state (exact match including hash). Items whose `href` contains `#` never match `pathname` — they stay inactive, which is intentional.
- `nav-data.ts` defines the sidebar groups/items statically. Many items link to hash-anchors within a real MDX page (e.g. `Copy mode → /docs/keybinds#copy-mode`) because the mockup shows finer-grained navigation than the actual MDX files offer.
