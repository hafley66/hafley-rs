# Brief: boop callers that hold a store or mail dir never resolve the default store

Defect: boop opened the real `~/.agent/boop.db` where the caller already had a
store or a `--mail-dir`. Found 2026-10-07:
- `deliver::land` and `supervise` read the mood through `Store::default_path()`
  (fixed in a4dca829: `supervise::mood_template_in`).
- A lane pane's own `boop` calls (`invoke::start` analytics, `beep lane delete
  --route-only --mail-dir X`) resolve the default store, not the lane's mail dir.

Scope: audit all 27 `Store::default_path()` call sites in crates/boop*/src.
Every caller that already holds a `Store` or a mail dir uses that instead.

Gate: `cargo test` and `cargo nextest run` on boop, boop-proc, boop-store,
boop-harness. Counts in the report.

Landing: commit on this branch. Do not push.

Rust workspace rule: `[profile.dev.package."*"] opt-level = 3`,
`[profile.dev] opt-level = 1`.
