# Brief: fix the ryi defects found dogfooding the v5-parity deslop

Evidence: plans/issues-ryi-dogfood-deslop-20261005.md on feature/ryi-v5-parity (merged before this
lane starts). Repo rules: hafley-rs CLAUDE.md (one implementation per concern; Rust fast resolver =
RustModuleIndex `hafley_scm/src/read/lang/rust_modules.rs`, slow = rust-analyzer; no workarounds or
fallback heuristics; abstain with a reason when the engine cannot answer).

## Method, per defect
1. Reproduce on the smallest fixture that shows it; commit the failing test first (whole-output
   snapshot, insta).
2. Root cause with file:line before writing a fix. State it in the commit message.
3. Fix at the root, in the existing engine. No second resolver, no text scan, no special case for
   hafley_scm. Prefer cargo / rust-analyzer / existing crate APIs over new code (e.g. workspace
   discovery = `cargo metadata` / `cargo_metadata` crate, not a hand walk of Cargo.toml files).
4. Diff stays small; delete code where possible. One commit per defect.

## Defects, in order
1. `ryii graph --callers <file>#<fn>` returns 0 rows for Rust fns with written intra-crate calls
   (29 queries in hafley_scm, e.g. `crates/hafley_scm/src/read/0c_arg_fields.rs#targets`,
   `closure_expr` in lang/rust/12_df_control.rs). Suspects to confirm or reject: `#[path = "..."] mod x;`
   modules, `use super::*` glob imports, methods called as `self.f()`. Fix in RustModuleIndex /
   the fast resolution step; check the slow tier (`--slow`) answers the same rows.
2. `ryii cleave` orphaned imports: plan lists 3 orphans (dispatch, flatten_jsonl, FamilyMask from one
   grouped `use sprefa_extract::{...}`) and removes only the first. All unused names must go; an
   emptied group deletes the whole `use`.
3. `ryii cleave` with `--root <outer workspace>` on a file of a crate that is its own workspace root
   (crates/sprefa-extract): `source ... is in no module of the Cargo workspace rust-analyzer loaded`.
   Load the workspace that owns the file (cargo metadata on the file's nearest manifest); when none
   owns it, abstain naming the manifest searched.
4. `ryii rename <path>#<sym> --root R`: path resolved under R a second time
   (`crates/sprefa-extract/crates/sprefa-extract/...`). Resolve positional paths the same way
   `ryii move` and `ryii cleave` do (one shared function; find it, do not add another).

5. Merging a file into an existing one is `ryii cleave --list` (every SRC#ITEM -> DEST, one stage);
   the lane reached for `move` instead. Gap: when the stage empties SRC, SRC stays on disk with its
   `mod`/`#[path] mod` declaration. A stage that leaves SRC with no items deletes SRC and its
   declaration. Test: merge tests/199_v5_parity-shaped file into an existing tests/all.rs-shaped
   file with one `cleave --list` run, no manual edits.

6. `ryii cleave 0c_call_owner.rs#covering_def 0d_flow.rs`: the plan rewrote a caller's import from the
   public reexport `crate::read::types::covering_def` to `crate::read::types::flow::covering_def`
   (`flow` is private), and reported no unresolved names. Callers that reach the item through a
   public reexport keep that path; a plan that writes a path the caller cannot see is an error.
7. `cleave --list` with `--json`: `error: the argument '--list <LIST>' cannot be used with '--json'`.
   Allow it (one JSON plan line per row).

Defect 1 clue: `ryii --kinds call 0c_arg_fields.rs` emits both the `objects` definition and the
written `objects` call at 242:249, yet `graph --callers objects 0c_arg_fields.rs` gives 0 rows, so even a
same-file call is missed.

`move` keeps refusing an existing destination; that refusal is correct.

## Proof
- Re-run every command from the issues file that hit these defects; table: command, rows_before,
  rows_after, wall_before, wall_after. Over ~10 rows -> scripts/bench_grid.py.
- For defect 1, rows_after must equal the written-call count from
  `ryii query` over the same files; list any difference with the reason.
- Full `cargo test --features cli --no-fail-fast` in crates/sprefa-extract, run in background;
  failures split into base (also on the start commit) and new.

## Limits
Commands under 2 min except the full suite. No whole-corpus runs. KACHE_DISABLED=1. No merge, no push.
