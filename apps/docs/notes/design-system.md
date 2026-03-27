# Design System

The docs site follows a **technical blueprint** aesthetic inspired by [Zed](https://zed.dev), with Agent Dash's warm orange brand identity.

## Colors

| Token | Dark | Light | Usage |
|-------|------|-------|-------|
| `--color-brand` | `#D97757` | `#D97757` | Primary accent, CTA buttons, active links |
| `--color-brand-dark` | — | `#c06040` | Primary accent in light mode (higher contrast) |
| `--color-sidebar` | `hsl(228, 15%, 6.5%)` | `hsl(228, 5%, 91%)` | Sidebar bg, diamond fills |
| `fd-background` | `hsl(228, 13%, 7.5%)` | default | Page background |
| `fd-border` | `hsl(228, 8%, 18%)` | default | Grid lines, borders, diamond strokes |

## Typography

- **Font**: Archivo (Google Fonts) — bold geometric grotesque, loaded via `next/font`
- **Headings**: `tracking-[-0.02em]` to `tracking-[-0.03em]`, `font-bold`
- **Code/terminal**: system monospace (`ui-monospace, SF Mono, Cascadia Mono, Menlo`)

## Structural Grid (Landing Page)

The grid is **structural**, not decorative wallpaper. Lines frame content sections:

- **Frame container**: `max-w-6xl` with `border-l border-r` creating vertical edges
- **Section dividers**: `GridLine` component renders `border-t` with `Diamond` SVG markers at each intersection
- **Ruler ticks**: repeating-linear-gradient along vertical edges (24px spacing), visible on `lg:` only
- **Dashed center line**: vertical dashed line at 50% width
- **Diamonds**: 9x9 SVG, rotated 45deg, filled with `--color-sidebar`, stroked with `--color-fd-border` at 1px
- **Mobile**: frame has `mx-4 sm:mx-6` padding; rulers and diamonds hidden below `lg:`

## Micro-Grid (Hero)

Subtle orange grid in the hero section, fading in from bottom:
- 24px cell size, `--color-brand` at 3% opacity
- CSS mask: `transparent 40% → black 100%` (bottom fade-in)

## Noise Texture

Paper-like grain applied globally via a fixed overlay:
- `/public/noise.webp` (from Zed's technique)
- `opacity: 0.012`, `background-size: 180px`, repeated
- Rendered in root `layout.tsx` as `<div className="noise-overlay" />`

## Fancy Button (AlignUI-inspired)

CTA buttons use a glossy effect via pseudo-elements:
- `::before`: 1px gradient inner border (white→transparent) using `mask-composite: exclude`
- `::after`: white→transparent gradient at `opacity: 0.16`, hover bumps to `0.24`
- Shadow: `inset 0 1px 0 rgba(255,255,255,0.12), 0 1px 3px rgba(0,0,0,0.2), 0 2px 8px rgba(217,119,87,0.25)`

## Border Radius Convention

Minimal radii to match the blueprint/technical aesthetic:
- Feature cards: `rounded` (4px)
- Terminal mockup, code blocks, video placeholder: `rounded-sm` (2px)
- Buttons: `rounded-md` (6px)
- Badges/pills: `rounded-full`

## Dark/Light Theme

- Theme toggle via `next-themes` (RootProvider handles it)
- Landing page nav has a `ThemeToggle` component (`src/components/theme-toggle.tsx`)
- Fumadocs docs layout has its own built-in toggle
- Terminal mockups and code blocks are always dark regardless of theme

## Docs Sidebar

Distinctly darker than the main content area (set via `#nd-sidebar` CSS override with `!important`).
