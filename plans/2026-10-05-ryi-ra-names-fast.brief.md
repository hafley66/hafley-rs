# Brief: fast Rust resolution on rust-analyzer module maps, warm in the daemon; retire RustModuleIndex

Source of truth: plans/2026-10-04-rust-resolution-unify.plan.md (inventory sections 1.1-1.4 and
"Decisions" at line 323). This brief executes decisions 2, 4, 5, 8, 1 and 6 in that order.
Generic first (CLAUDE.md): one module-place provider used by graph, cleave, move, rename and every
syntax resolve arm. No language- or command-specific copies.

## Type signatures (pseudo)
```rust
/// One loaded rust-analyzer database in Names mode (workspace crates, no sysroot, no inference).
struct NamesHost { db: RootDatabase, crates: Vec<Crate>, key: ManifestKey }
/// Cache key: content ids of every Cargo.toml + Cargo.lock in the workspace (decision 8).
struct ManifestKey(Vec<(PathBuf, ContentId)>);
/// The single question fast callers ask.
fn module_places(host: &NamesHost, file: &Path) -> Result<Vec<ModulePlace>, Abstain>;
fn resolve_path(host: &NamesHost, file: &Path, path: &[Name]) -> Result<Vec<DefPlace>, Abstain>;
/// Method calls: Names cannot type a receiver -> Abstain::NeedsTypes (never a name guess).
```

## Instance lifetimes
- CLI one-shot: load a NamesHost for the owning workspace (cargo metadata on the file's nearest
  manifest; one shared function, replacing both copies added on 2026-10-05:
  `0_rust_module_context.rs` and `8e_rust_checker_modules.rs`/`1h_rust_module_tree.rs`), answer,
  drop.
- Daemon: hosts live in a map keyed by ManifestKey; a changed manifest content id evicts and reloads;
  source edits apply as rust-analyzer file changes (no reload). Provider sits beside body_edges
  (decision 5).

## Storage / reads / writes
- Reads: VFS from disk for workspace crates; no sysroot, no deps' sources unless the path resolves
  into a dep (then Abstain::OutsideWorkspace or a dep stub, decided by the plan's 1.3 table).
- Writes: module places as store relations with contract views (decision 3) only if already
  scheduled there; otherwise in-memory answers. Uniqueness: (ManifestKey, file content id).

## Work, one commit each, failing test first
1. Measure Names load on hafley-rs and sprefa-extract once (cold, then warm in the daemon): table.
2. Shared workspace discovery function; both 2026-10-05 copies call it.
3. Names provider + daemon cache; rename the rust-analyzer load enum so fast/slow mean no-types/types
   (decision 4; remove Tier::Fast).
4. Switch every RustModuleIndex caller (inventory 1.1: graph arms, 1_type.rs, 2_call.rs, cleave's
   rust_route_index) to the provider. Method calls abstain with `needs_types` unless --slow.
5. Agreement run: old index vs provider on hafley-rs crates (callers/uses/module places), single run;
   every disagreement listed with file:line and which side is right.
6. Delete RustModuleIndex and its helpers (rust_modules.rs) and update the CLAUDE.md line naming it.
   Tests: fold into table-driven fixture tests (CLAUDE.md test rule).

## Limits
`[profile.dev.package."*"] opt-level = 3` is set. KACHE_DISABLED=1. Commands under 2 min except test
gates (uncapped, CLAUDE.md). No whole-corpus loops; one measurement run each. No merge, no push.
