---
name: verify
description: Run clippy and tests to verify code quality before pushing. Use after making changes or before creating a PR.
---

## Clippy (`cargo clippy -- -D warnings`)
!`cargo clippy -- -D warnings 2>&1`

## Tests (`cargo test`)
!`cargo test 2>&1`

If there are errors above, fix them and re-run the failing checks until they pass. Report results concisely.
