# TS 7.1 beta API vs ryi fast (user-set 2026-10-29)

User's ask: "tsc7.1 beta allows api access to semantic/ast analysis calls, so what can we do with that after
testing speed of each on large ts projects vs fast"

- Package: `typescript@7.1.0-beta` (npm `beta` tag).
- Corpora: TypeScript repo src, VS Code, the existing bench corpora under `crates/sprefa-extract/bench/`,
  the user's own repos.
- API calls timed: all of them.
- Baselines: `ryii fast`, `ryii fast --resolve`. The existing slow tier (ts-checker / ts7 LSP) only if it works;
  "i dont need it if it doesnt work".
- Measurement: median of 3 runs.
- Deliverable: report only.
- Writes allowed: scratch dirs, `crates/sprefa-extract/bench/` (corpora and scripts), and the report in
  `crates/sprefa-extract/plans/`.
- "do not make git submodules be in our repo": corpus clones under `bench/` stay plain untracked checkouts, never submodules.
