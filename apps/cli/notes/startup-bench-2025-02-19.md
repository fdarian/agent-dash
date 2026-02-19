# Startup Benchmark — 2025-02-19

## Environment

- Runtime: Bun
- Entry: `entries/cli.ts`
- Module count (unbundled): 956 across 4 heavy deps (`effect`, `@effect/cli`, `@effect/platform-bun`, `@opentui/core`)
- App's own code: ~5ms (negligible)
- Bottleneck: module resolution at runtime (~287ms / 75% of startup)

## E2E Startup (hyperfine --warmup 3 --runs 10)

| Approach | E2E mean | Min | vs baseline |
|----------|----------|-----|-------------|
| Unbundled (baseline) | 381ms | 333ms | — |
| **`bun build --external @opentui/core`** | **258ms** | **202ms** | **-32%** |
| Bundle + `--minify` | 275ms | 196ms | Same as bundle |
| Bundle + `--bytecode` | Broken | — | opentui tree-sitter |
| `--compile` (standalone binary) | 272ms | 200ms | Same as bundle |

**Winner**: `bun build --target=bun --outdir=dist --external @opentui/core`

## Subpath Imports (isolated, dev mode)

| Package | Barrel | Subpath | Speedup |
|----------|--------|---------|---------|
| `effect` (11 symbols) | 105ms | 54ms | **49%** |
| `@effect/platform-bun` (2 symbols) | 160ms | 104ms | **35%** |
| `@effect/cli` (2 symbols) | 130ms | 129ms | ~0% (skip) |

## Decisions

- Bundle with `@opentui/core` external (native FFI can't be bundled)
- Subpath imports for `effect` and `@effect/platform-bun` (faster dev mode)
- Skip subpath for `@effect/cli` (no measurable benefit)
- Skip `--minify` (no additional benefit)
- Skip `--bytecode` (breaks opentui tree-sitter)
- Skip `--compile` (same perf as bundle, less flexible)
