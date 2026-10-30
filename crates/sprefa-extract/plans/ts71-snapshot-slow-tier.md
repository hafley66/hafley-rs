# TS slow tier = TS 7.1 snapshot (user-set 2026-10-29)

Two tiers, fast and slow, nothing else. TSI is the wire both write. SCIP is the symbol/conformance spec
TSI bridges to (`tsi.scip_symbol`); it stops being a TS producer.

| tier | TS | Rust |
| --- | --- | --- |
| fast | oxc / tree-sitter, module plane | syn / tree-sitter |
| slow | TS 7.1 snapshot (`tsgo --api`) | rust-analyzer library (unchanged) |

Removed for TS: `ts-checker` cargo feature, `ts_checker.rs` + `ts_checker.mjs` (node tier), the ts7 LSP session
(`0_ts7_lsp_session.rs`, `edit/1a_ts7_lsp_session.rs`), `1_tsgo_rows.rs`, `ScipTypescript` producer
(`scip.rs`, `scip_ensure.rs`, `ScipMode::{Build,Load}` for TS). The edit arms (`edit/1_ts7_rename.rs`,
`1b_ts7_symbol_seed.rs`, `1c/1d_ts7_cleave_*`, `1e_ts7_graph_target.rs`, `1g_ts7_resolve.rs`) move onto the
same snapshot: one implementation per concern. Rust SCIP (`59_rust_scip_macros`, `scip` verb for Rust) is a
separate decision, untouched here.

Facts this plan stands on (measured 2026-10-29, `plans/ts71-api-bench.md`, TS 5.9 `src`, 601 files):
open projects 124 ms / 295 MB; 99,542 call-site symbols batched 1.5 s; snapshot memory is never released
until `dispose`/`close`; `GOMEMLIMIT=1GiB` + `GOMAXPROCS=2` lowered peak 1,489 -> 1,463 MB and cost 36%
wall time (live data, not garbage). Server: `tsgo --api --cwd <root>` over stdio or `--pipe <path>`
(`dist/api/options.js`, `syncChannel.js`); protocol `dist/api/proto.js`. The beta has panics recorded in
the bench report (`formatNodeForInsertion`, some `transpile*`, `getImmediateAliasedSymbol`).

## 1. Type signatures

```rust
// hafley_scm/src/read/lang/ts71/0_proto.rs — wire types generated or hand-mirrored from dist/api/proto.js
pub struct ProjectHandle(pub u64);
pub struct SymbolHandle(pub u64);
pub struct TypeHandle(pub u64);

// ts71/1_server.rs — one tsgo --api child
pub struct Ts71Server { /* child, stdin/stdout framing, request ids */ }
impl Ts71Server {
    pub fn spawn(root: &Path, tsgo: &Path) -> Result<Self, Ts71Error>;
    fn call<P: Serialize, R: DeserializeOwned>(&mut self, method: &str, params: &P) -> Result<R, Ts71Error>;
    pub fn close(self) -> Result<(), Ts71Error>;
}

// ts71/2_snapshot.rs — one checked program state; dropped == disposed
pub struct Snapshot<'s> { server: &'s mut Ts71Server, id: u64, projects: Vec<ProjectHandle> }
impl<'s> Snapshot<'s> {
    pub fn open(server: &'s mut Ts71Server, tsconfigs: &[PathBuf]) -> Result<Self, Ts71Error>;
    pub fn update(&mut self, edits: &[(PathBuf, String)]) -> Result<Snapshot<'_>, Ts71Error>;
    pub fn symbols_at(&mut self, file: &Path, offsets: &[u32]) -> Result<Vec<Option<SymbolHandle>>, Ts71Error>;
    pub fn aliased(&mut self, symbols: &[SymbolHandle]) -> Result<Vec<SymbolHandle>, Ts71Error>;
    pub fn declarations(&mut self, symbols: &[SymbolHandle]) -> Result<Vec<Vec<DeclSite>>, Ts71Error>;
    pub fn types_at(&mut self, file: &Path, offsets: &[u32]) -> Result<Vec<Option<TypeHandle>>, Ts71Error>;
    pub fn diagnostics(&mut self, file: &Path) -> Result<Vec<Diagnostic>, Ts71Error>;
}
impl Drop for Snapshot<'_> { fn drop(&mut self) { /* dispose: frees the server's handles */ } }

// ts71/3_rows.rs — slow TSI for TS, through the existing trait
pub struct Ts71Rows { facts: Vec<FactOut>, coverage: Vec<CoverageClaim> }
impl SemanticRows for Ts71Rows { fn facts(&self) -> &[FactOut]; fn coverage(&self) -> &[CoverageClaim]; }
pub fn slow_ts(fast: &ProjectFacts, tsconfigs: &[PathBuf], budget: &SlowBudget) -> Result<Ts71Rows, Ts71Error>;

pub struct SlowBudget { pub deadline: Option<Instant>, pub cancel: Option<Arc<AtomicBool>>, pub max_rss_mb: Option<u64> }
```

## 2. Bodies (pseudo)

```rust
fn slow_ts(fast, tsconfigs, budget) -> Ts71Rows {
    // demand comes from fast: only sites fast left unresolved or ambiguous, never every identifier
    let demand: BTreeMap<File, Vec<Offset>> = fast.unresolved_call_sites().group_by_file();
    let mut server = Ts71Server::spawn(root, tsgo)?;               // one per slow run
    let mut rows = Ts71Rows::default();
    for tsconfig in tsconfigs {                                      // one snapshot per project, sequential
        let mut snap = Snapshot::open(&mut server, &[tsconfig])?;
        for (file, offsets) in demand.files_owned_by(tsconfig) {
            budget.check()?;                                         // deadline, cancel, server RSS
            let symbols = snap.symbols_at(file, offsets)?;           // one batched round trip per file
            let targets = snap.aliased(symbols)?;                    // through re-exports to the declaration
            let decls   = snap.declarations(targets)?;
            rows.push_calls(file, offsets, decls);                   // tsi.called + tsi.scip_symbol minted from decl
        }
        rows.push_coverage(tsconfig, demand.covered(tsconfig));
        drop(snap);                                                  // dispose before the next project opens
    }
    server.close()?;
    rows
}
```

Edit arms: `Snapshot::update` with the edited texts, then `diagnostics` per touched file (rename/cleave
verification), then drop. No disk write until the verify passes.

## 3. Lifetimes

```
slow run   |--server spawn---------------------------------------------close--|
project A      |--snapshot A open--batches--dispose--|
project B                                              |--snapshot B--dispose--|
edit verify          |--update(A) open--diagnostics--dispose--|   (nested in A's lifetime)
```

- `Ts71Server`: one per slow run (CLI process) or per daemon request; never shared across requests.
- `Snapshot`: one per tsconfig, sequential; peak memory = server base + largest single project, not the union.
- Handles (`SymbolHandle`, `TypeHandle`): valid only inside their snapshot; never stored past its drop.
  Cross-project handle use fails in the beta (`type handle not found in project registry`), so no query
  mixes handles from two snapshots.

## 4. Storage, reads, writes

- Reads: fast `ProjectFacts` (unresolved sites, file -> owning tsconfig), source text from the same blob
  set fast read (content ids must match, the mission's admission rule).
- Writes: TSI rows only (`emit_semantic` appends after fast; ordinals continue, ids renumber), one
  `CoverageClaim` per tsconfig (`complete` when every demanded site answered, else `partial` with counts).
- Uniqueness: one `tsi.called` per (file, call site span); `tsi.scip_symbol` keyed by declaration
  (package, file, descriptor chain); a site answered by two projects keeps the owning tsconfig's answer.
- Budget: `deadline::within_or_cancel` wraps the run (same timer as query/graph); the server RSS is
  sampled per batch (hafley-observe `rusage` / process sample) and past `max_rss_mb` the run stops with a
  partial coverage claim and the reason, never a guess.

## 5. Receipts (hafley-observe growth_bench)

- Count: demanded sites answered + RPC round trips, exponent <= 1.1 over files.
- Time: median of 3 per size, exponent <= 1.3; server peak RSS recorded per size.
- Equality: slow call edges vs the old `ts-checker` / `scip-typescript` rows on the TS 5.9 fixture set before
  those paths are deleted; differences listed, not hidden.

## 6. Order of work

1. `ts71/0_proto.rs` + `1_server.rs` + `2_snapshot.rs` (spawn, open, symbols_at, dispose) and one test over a fixture.
2. `3_rows.rs`: slow TS call edges through `SemanticRows`; equality vs old paths; growth bench.
3. Route `ryii --resolve` slow and `graph --slow` to it; delete `ts-checker`, node tier, LSP session, tsgo_rows,
   `ScipTypescript`.
4. Edit arms onto `Snapshot::update` + `diagnostics`; delete their LSP paths.

## 7. Memory: tsc vs the calling pattern (measured 2026-10-30)

`bench/scripts/11_ts71_memory.mjs`, typescript 7.1.0-beta, TS 5.9 `src/compiler/tsconfig.json` (222 files):

| phase | server RSS |
| --- | --- |
| `tsc --noEmit --incremental false` (full check, separate process) | 322 MB peak, 0.51 s |
| API spawn -> snapshot open | 13 -> 154 MB |
| AST fetch, 77 corpus files | 242 MB |
| `getSymbolAtPosition` on 364577 identifiers | 844 MB |
| same pass again | 881 MB |
| snapshot dispose, +3 s idle | 881 MB |
| new snapshot, full pass again | 887 -> 903 MB |
| `GODEBUG=madvdontneed=1`, same sequence | 808 -> 966 MB |

- The checker's own state for the whole program: ~322 MB.
- The per-identifier sweep adds ~600 MB over the open snapshot; that is the calling pattern.
- Dispose does not lower RSS; the next snapshot's full pass reuses it (+6 to +22 MB). Freed
  Go heap stays resident and is reused; RSS is the high-water mark, not a leak.
- Cap: demand-driven sites only (section 2), and restart the server past `max_rss_mb`
  (spawn + open = ~140 ms here) instead of waiting for dispose to return memory.
