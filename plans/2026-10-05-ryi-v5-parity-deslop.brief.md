# Brief: three deslop passes on feature/ryi-v5-parity (a44eced8..e5df3cae)

Branch diff today: src +3043/-2452 (20 files), tests +647 (17 files), snapshots +14487 (12 files).
Behavior stays identical: every pass ends with the 8 `v5_parity` tests plus t_13_flow_join and
t_12_df_identity green with NO snapshot change, except where a pass deletes or shrinks a snapshot on
purpose. One commit per pass. Report per pass: lines removed, lines added, files deleted.

## Pass 1: code
- Delete dead code, unused params/fields, defensive branches that cannot fire, comments that narrate
  the code or cite tickets/v5 history.
- One implementation per concern: 0c_arg_fields.rs, 0c_closure_flow.rs, 0c_call_owner.rs,
  ts/4_call_closure.rs, ts/3_df_jsx.rs, 0d_flow.rs share span-containment, owner lookup and edge
  push helpers; keep one copy each. Fold a file into its caller when it holds one small function.
- `df_owner` (ts/0_df_rows.rs) builds `{file}::function::{name}` then strips it: pass the name.
  Same for any other format-then-parse round trip.
- No `.iter().find()` wrappers or one-line helpers around array methods.
- File splits made in this branch (ts.rs, types.rs, 11_df_syntax_rows.rs, 0_sqlite.rs) stay
  pure moves: diff them against base and undo any edit hidden in a move.

## Pass 2: repo debris and test layout
- Remove committed logs/receipts from plans/: 0_ryi_v5_parity_errors.txt, and the *.log files.
  Keep plans/1_ryi_v5_parity_receipt.md only if the coordinator asks; default delete.
- Move the separate `v5_parity` [[test]] target into tests/all.rs modules; drop the Cargo.toml
  [[test]] entry. One test binary.
- Tests share setup through one helper; no per-test copies of dispatch/FamilyMask boilerplate.

## Pass 3: snapshots
- 14,487 snapshot lines. Replace whole-output dumps with projections that carry the claim:
  edge lists as `from_kind span -> to_kind span` lines, proof tables as in deferred_jsx_proofs.
  Keep one whole-output snapshot per fixture family at most. Target under 2,000 snapshot lines.
- Closure names embed the file content hash (`closure@App:blake3:<64 hex>:0`); use the closure span
  instead (`closure@App:<start>`), so names survive unrelated edits. This is the one allowed output change;
  snapshots move with it.

## Limits
Same as plans/2026-10-05-ryi-v5-parity.brief.md. Commands under 2 min. No merge, no push.
After pass 3, run `cargo test --features cli --no-fail-fast` in crates/sprefa-extract in the
background (it takes longer than 2 min) and report its failure list exactly; do not fix failures that also
fail on a44eced8 — list them as base failures.
