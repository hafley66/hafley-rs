# Lane: tiny cleanup batch 1 (hafley-rs)

Favor plain code. Work the items in order; one commit per item. If an item's premise is already false on this tree, skip it, set its issue `status: fixed` with a one-line `## Tests Run` note naming the commit or file that already did it, and move on. If anything else deviates, STOP and write REPORT.md.

## Items, in order

1. **boop-temprepo-dedupe** (issues/boop-temprepo-dedupe/item.md). Every acceptance box is already ticked and one `TempRepo` lives at crates/boop-store/src/testing.rs:12. Set `status: fixed`, note the file. No code.

2. **boop-rfc3339-parser-dedupe** (issues/boop-rfc3339-parser-dedupe/item.md). The only parser is `iso_to_ms` at crates/boop-harness/src/transcript.rs:63 (neutral file). Grep `crates` for any other hand-rolled RFC-3339 parse (`fn .*rfc3339`, `split('T')`, `parse::<u64>` on timestamp strings). If none: `status: fixed`, note it. If one exists: replace its body with a call to `iso_to_ms` and add a test.

3. **boop-one-shell-quote** (issues/boop-one-shell-quote/item.md). Four identical single-quote fns: crates/boop-harness/src/harness.rs:506 `quote`, crates/boop-harness/src/harness/claude.rs:566 `shell_quote`, crates/boop-harness/src/harness/opencode.rs:1003 `shell_quote`, crates/boop-mux/src/lib.rs:632 `quote_arg`. Make `harness.rs::quote` `pub fn shell_quote` in boop-harness, delete the claude.rs and opencode.rs copies, point boop-mux at it only if boop-mux already depends on boop-harness (check Cargo.toml; if it does not, leave boop-mux alone and say so). Leave crates/boop/src/cli/job.rs:1148 alone: another lane owns job.rs today. `opencode.rs:1010 shell_quote_double` stays (different job).

4. **boop-dead-code-allows** (issues/boop-dead-code-allows/item.md). Four crate/module-level allows: crates/boop-mux/src/lib.rs:10, crates/boop-harness/src/harness/{codex.rs:3,kimi.rs:7,claude.rs:2}. Remove each, run clippy, and for every warning that appears either delete the dead item or put a `#[allow(dead_code)]` with a one-line reason on that item alone. Do not touch boop-store/src/event.rs.

5. **boop-registry-into-sqlite** (issues/boop-registry-into-sqlite/item.md). Routes already live in `agent_route` (168 rows live) and `read_routes` (crates/boop-store/src/bus.rs:65) reads the store. `write_route` (:70) still writes `registry.json` and relies on `import_legacy` (:519) at the next open. Make `write_route` upsert `agent_route` directly through the store (there is an existing insert path used by `import_legacy`; reuse it), keep `import_legacy` for old mail dirs, fix the stale module doc at bus.rs:3, and rename or replace `sha256_hex` per the issue. `cas_update_json` stays if `lane-residency.json` or `parent-policy.json` still use it. Tests in deliver.rs (:1035, :1127, :1132, :1299) and control.rs/me.rs callers keep compiling unchanged.

6. **boop-kind-enums** (issues/boop-kind-enums/item.md), only if items 1 to 5 are committed and green. `Message.kind` and `Route.kind` at crates/boop-store/src/bus.rs:26 and :39 become enums with serde rename to the wire strings plus `Other(String)` for unknown rows; follow the issue's acceptance list.

## Files you own
crates/boop-harness/src/harness.rs, crates/boop-harness/src/harness/{claude,codex,kimi,opencode}.rs, crates/boop-mux/src/lib.rs, crates/boop-store/src/bus.rs, crates/boop-store/src/ident.rs only for the route upsert helper item 5 needs, the six issue files. Nothing in crates/boop/src (another lane owns job.rs and supervise.rs today) except compile-fix fallout from item 6, which you list in REPORT.md.

## Rules
- Commit subjects: `boop-harness: one shell_quote`, `boop: dead_code allows off, per-item reasons`, `boop-store: write_route upserts agent_route; registry.json import-only`, `boop-store: Message.kind and Route.kind are enums`.
- Banned identifiers: provenance, substrate, load-bearing, regime.
- No behavior change beyond the item. No reformatting outside touched lines.

## Validation (run after every item, paste the final run into REPORT.md)
```
export CARGO_TARGET_DIR=$HOME/.cache/cargo-target/tiny-1
cargo test --workspace 2>&1 | grep -E '^test result|FAILED|panicked' | head -30
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5
git log --oneline -7
```
Known env-only failure to report, not fix: `deliver_door::a_route_with_a_live_pane_takes_the_paste_rung`.
Write REPORT.md at the worktree root: items done, items skipped with why, test counts.
