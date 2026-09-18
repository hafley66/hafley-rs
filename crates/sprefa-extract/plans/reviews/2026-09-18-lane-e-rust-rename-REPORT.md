# Lane E-2: rust rename seats, WIP resume

## Rows

| gap | status at WIP hand-off | after repair |
| --- | --- | --- |
| E.1 `#[path]` | full (static read) | full, confirmed by `path_attr_places_the_file_and_renames_its_seats` |
| E.2 field and variant | **broken cross-file**: `harvest()` gated every Field/Variant seat site behind `anchored` (`Option<&Decl>`, `Some` only in the anchor's own file), and `owner_reach`'s prefix contract didn't match `FieldSite::Owner`'s prefix shape | fixed, see `## Fixes beyond the WIP` |
| E.3 serde and string spellings | full (static read) | full, confirmed by `serde_literal_is_reported_as_a_text_ref` |
| E.4 fn-body `use` | full (static read) | full, confirmed by `fn_body_use_scopes_the_bare_name_to_its_block` |

A static read of the WIP (2173 lines, all four gaps present in shape) said all four were done. Fixture + scip-verify runs proved E.2 was silently dropping every field/variant seat outside the anchor's own declaring file — the majority case for a real rename, since a struct's usages live mostly in OTHER files. Static read is not a substitute for the fixture/scip proof the brief asks for; this is why.

## Fixes beyond the WIP

| defect | file:line | fix |
| --- | --- | --- |
| Field/Variant seat detection (`binds_variant`, the path-loop `owner_reach` call, the whole `DeclKind::Field` block) was gated on `anchored: Option<&Decl>`, `Some` only when the current file IS the anchor file. `h.size`/`Helper{size}`/`Kind::Old` in every OTHER file were silently dropped. | `rust_rename.rs` `harvest()`, was ~422/456/521/569 | Added `anchor_kind: &DeclKind` and `anchor_module: &ModuleId` as always-populated `harvest()` params (computed once in `symbol_refs`, not derived from `anchored`); the three call sites now branch on `anchor_kind` directly. `anchored` keeps its narrower role: per-file decl shadowing bookkeeping, and the pre-existing (unchanged, out of scope) Method-only macro-body-stop and `.old()`-call gates. |
| `owner_reach(chain, prefix, owner, ...)` requires `prefix.split_last().0 == owner` (the variant-path shape: `path.prefix` for `Kind::Old` already ends in `Kind`). `FieldSite::Owner.prefix` is the segments BEFORE the owner (`push_owner`: `prefix: segments[..len-1]`), so for a bare `Helper { size }` (prefix `[]`) `owner_reach` always hit `prefix.split_last() == None` and returned false. | `rust_rename.rs` field-anchor block, was ~586 | New `owner_path(prefix, owner) -> Vec<String>` appends `owner` before the call, giving `owner_reach` the shape it expects. |

Both were caught only by the new fixtures: `field_seats_rename_through_the_receiver_plane` (lib.rs seats, a different file than the anchor `util.rs`) and `scip_verify_agrees_on_the_variant_fixture`/`scip_verify_agrees_on_the_field_fixture` (rust-analyzer disagreed on exactly the missed cross-file/struct-literal spans before the fix, agreed after).

## Commits

| sha | subject | files |
| --- | --- | --- |
| `3716106e` | `feat(extract): rust rename places #[path] modules and scopes fn-body use` | `crates/sprefa-extract/src/lang/rust_rename.rs` |
| `32021e48` | `test(extract): rust rename fixtures for path, field, variant, serde, fn-body use` | `tests/5_rename_rust.rs`, `tests/fixtures/rust_rename/{path,field,field_stop,variant,serde,fnuse}/**` (41 files) |

Deviation from the brief's four-commit list: the WIP's `DeclKind` refactor (replacing `Decl.method: bool`) is read by E.2's field/variant logic AND by E.4's `member_anchor`/final-method-seat arms in the same `harvest()` function; splitting the diff into separate compiling commits per gap (`feat: field and variant seats`, `feat: serde and string spellings`) risked landing an intermediate commit that doesn't build. One `feat` commit carries all three subjects' content (path+fn-body-use, field+variant, serde+text) rather than three commits gamed with empty or misattributed diffs. The subject used is the brief's own first-listed one, which already pairs E.1+E.4; the body names all three gaps explicitly.

## Seats

| fixture | case | seat span | form | role | outcome |
| --- | --- | --- | --- | --- | --- |
| path | `#[path]`-placed decl, `use`, body ref in `lib.rs` | 3 spans | — | Definition/Import/TypeRef | renamed |
| path | `src/other.rs` `Helper` | — | — | — | untouched (different module) |
| field | `Helper.size` decl, `self.size`, `h.size` (param-typed), `h.size`(let-typed), literal key, pattern key | 6 spans | — | Definition/Write/Read | renamed; pattern key respells `size` -> `width: size` (shorthand) |
| field | `Other.size` decl, `o.size` | — | — | — | untouched (different owner) |
| field_stop | `v.size` where `v`'s type is Unknown (`make()` declared in another file, outside the one-hop same-file rule) | 1 span | `untyped field` | — | `RenameStop::Dynamic`, exit 6, tree untouched |
| variant | `Kind::Old` decl, `Kind::Old` in `make()`/match arm, `use crate::Kind::Old;`, bare `Old` in `via_use()` | 5 spans | — | Definition/Read/Import | renamed |
| variant | `mod other { enum Kind { Old } }` | — | — | — | untouched (different enum) |
| serde | `Helper.size` decl, `h.size` | 2 spans | — | Definition/Read | renamed |
| serde | `#[serde(rename = "size")]` literal, `S.len` | — | — | — | untouched; literal reported once via `--text-refs`, never rewritten |
| fnuse | `use crate::util::Helper;` (in `fn a`'s block), `Helper::new()` (in `fn a`) | 2 spans | — | Import/Read | renamed, scoped to `fn a`'s block |
| fnuse | module-scope `struct Helper;`, `fn b`'s two spellings | — | — | — | untouched (fn-body `use` never shadows outside its block) |

## Verify

### cargo check, per `after/` crate (`new_fixture_crates_pass_cargo_check`)

| crate | result |
| --- | --- |
| path/after | pass |
| field/after | pass |
| variant/after | pass |
| serde/after | pass (offline, `Cargo.lock` pinned to the already-cached serde 1.0.229) |
| fnuse/after | pass |

`field_stop` has no `after/` (stop, tree untouched, nothing to check).

### scip-verify (rust-analyzer as the E.2 oracle)

| fixture | rows | index build time |
| --- | --- | --- |
| field | `scip-verify disagreements=0` | 2.8 s (MEASURED 2026-09-18) |
| variant | `scip-verify disagreements=0` | 2.2 s (MEASURED 2026-09-18) |

Both under the 10 s cap; no `#[ignore]`. Before the E.2 cross-file fix, field showed `disagreements=4` (4 `scip-only` misses in `lib.rs`) and variant showed `disagreements=2` (2 `scip-only` misses in `uses.rs`); after the fix, both agree.

## Tests changed

| test | old expectation | new expectation | why |
| --- | --- | --- | --- |
| `path_attr_places_the_file_and_renames_its_seats` | did not exist | `#[path]`-placed file's decl/use/body seats rename; unrelated same-named struct in another file stays | E.1 |
| `field_seats_rename_through_the_receiver_plane` | did not exist | decl, `self.x`, param-typed access, let-typed access, literal key, shorthand pattern key all rename; different-owner field of the same name stays | E.2 |
| `untyped_field_access_is_a_dynamic_stop` | did not exist | an access typed only through a cross-file fn return is `RenameStop::Dynamic`, form `untyped field`, tree untouched | E.2 |
| `variant_seats_rename_through_owner_and_use` | did not exist | bare path, match arm, `use` clause, and the bare name it binds all rename; a same-named variant in another enum stays | E.2 |
| `serde_field_seat_renames_and_leaves_the_literal` | did not exist | field decl + access rename; `#[serde(rename=..)]` literal stays byte-identical | E.3 |
| `serde_literal_is_reported_as_a_text_ref` | did not exist | `--text-refs` reports the literal exactly once, uncommitted | E.3 |
| `fn_body_use_scopes_the_bare_name_to_its_block` | did not exist | fn-body `use` + its call site rename inside the block; a same-named module-scope item outside the block keeps its own spelling | E.4 |
| `new_fixture_crates_pass_cargo_check` | did not exist | rustc accepts all 5 new `after/` crates | brief step 2 |
| `scip_verify_agrees_on_the_field_fixture`, `scip_verify_agrees_on_the_variant_fixture` | did not exist | rust-analyzer and the plan agree, 0 disagreements | brief step 3, E.2 oracle |

The 7 pre-existing rust rename tests (`5_rename_rust.rs`), `3_move_rust.rs` (17), `71_rust_paths.rs` (8), and `golden_parity.rs` (11) are unchanged in expectation and stayed green throughout.

## Gate

```
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
Doc-tests sprefa_extract
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Full run: `cargo test --features cli --no-fail-fast` — 180 test binaries reporting `test result: ok`, 0 reporting `test result: FAILED` (checked via `grep -c` over the full log, not by eye).

## Blocked

(empty — no command failed in a way this brief did not anticipate)
