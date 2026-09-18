# Lane K1: kotlin phase-1 receiver plane and the `receiver` leg: REPORT

Worktree `feature/extract-lane-k1-kotlin-receivers`, base sha `cb496088`. Scope: kotlin only. Gate binary: `$CARGO_TARGET_DIR/debug/extract`, CTF db `$CARGO_TARGET_DIR/ctf.db` (base) and `ctf-head.db` (HEAD).

## Commits

| sha | subject | files |
| --- | --- | --- |
| 78520205 | feat(extract): kotlin phase 1 mints method owners and receiver bindings | `crates/sprefa-extract/src/lang/kotlin.rs` (`def_span` visibility, `project_call` blob + walk hook), `crates/sprefa-extract/src/lang/kotlin_receivers.rs` (new), `crates/sprefa-extract/src/lang/mod.rs` (one `pub mod` line) |
| 7be00f7c | feat(extract): kotlin receiver leg binds members through the corpus owner table | `crates/sprefa-extract/src/lang/kotlin.rs` (receiver leg, one-hop field upgrade, `call_drops`, `receiver_target`/`field_type_of`/`module_type_file`), `crates/sprefa-extract/src/lang/kotlin_receivers.rs` (scope/type fixes), `crates/sprefa-extract/src/project.rs` (one line: kotlin arm `drops`), `crates/sprefa-extract/tests/131_kotlin_module_resolve.rs`, `crates/sprefa-extract/tests/137_kotlin_receiver_legs.rs` (new), `crates/sprefa-extract/tests/fixtures/kotlin_receivers/lib.kt` (new), `crates/sprefa-extract/tests/fixtures/kotlin_receivers/use.kt` (new) |
| 7dc7b1a7 | fix(extract): kotlin shadowing reads the receiver plane, not df | `crates/sprefa-extract/src/lang/kotlin.rs` (deletes `KotlinSource::shadowed` and its resolve call, 31 lines) |

| note | fact |
| --- | --- |
| `src/project.rs` is outside the owned-file list | One line: the kotlin `ResolveArm` row's `drops: None` became `drops: Some(crate::lang::kotlin::call_drops)`. Without it the `unresolved` channel is unreachable for kotlin and the acceptance criterion "declines with drop reason inferred" has no seat. Lane D's `rust_modules.rs` edit is the precedent. |
| Fixture syntax | The brief's fixture text `fun boundLeg<P : Proj>(p: P)` is invalid Kotlin (type parameters precede the function name); both grammars error-recover it into a nameless def. The fixture spells the valid `fun <P : Proj> boundLeg(p: P) = p.project()`; the leg semantics are unchanged. |

## Legs

Fixture: `tests/fixtures/kotlin_receivers/{lib,use}.kt`. Expected values hand-derived from the fixture text (def starts are byte offsets of the `fun` keywords).

| leg | expected | observed | origin |
| --- | --- | --- | --- |
| param (`fun paramLeg(w: Widget) = w.run()`) | edge to `Widget.run` (lib.kt, callee_start 323), NOT `Decoy.run` (479) | EDGE `paramLeg run lib.kt 323` | `receiver` |
| ctor (`val w = Widget(2); w.run()`) | edge to `Widget.run` (323) | EDGE `ctorLeg run lib.kt 323` | `receiver` |
| return (`val w = makeWidget(); w.run()`) | unresolved, reason `inferred`, zero edges to any `run` (ctor-return through a fn's declared return type is not a K1 leg) | DROP span 503-508, reason `inferred`, detail `run`; no `returnLeg` `run` edge | - |
| field (`fun fieldLeg(h: Holder) = h.w.run()`) | one-hop `h.w` upgrades to `Named(Widget)` through lib.kt's written type, edge to `Widget.run` (323) | EDGE `fieldLeg run lib.kt 323` | `receiver` |
| bound (`fun <P : Proj> boundLeg(p: P) = p.project()`) | `p` binds the single bound `Proj`; edge to `Proj.project` (lib.kt, 439) | EDGE `boundLeg project lib.kt 439` | `receiver` |
| object (`fun objectLeg() = Gadget.spin()`) | unbound uppercase receiver names the object; edge to `Gadget.spin` (lib.kt, 395) | EDGE `objectLeg spin lib.kt 395` | `receiver` |
| this / implicit (`class Inner { fun self() = run() }`) | bare `run()` inside the class body is `this.run()`; edge to `Inner.run` (use.kt, 706) | EDGE `self run use.kt 706` | `receiver` |
| shadow (`fun shadow(run: () -> Int) = run()`) | the param is scope-bound (a function type, untyped); zero edges to any `run`; drop reason `inferred`, no df plane read | DROP span 752-755, reason `inferred`, detail `run`; no `shadow` `run` edge | - |

Cross-check: `run` has three corpus member defs (`Widget.run` 323, `Decoy.run` 479, `Inner.run` 706). Every bound site carries the exact `callee_start`; the 4 receiver `run` edges all name 323 or 706, never 479.

## CTF

Corpus: `tests/fixtures/kotlin/*.kt tests/fixtures/kotlin_module_resolve/**/*.kt` at base (the receivers fixtures do not exist at base), plus `tests/fixtures/kotlin_receivers/*.kt` at HEAD. Base sha `cb496088`, HEAD sha `7dc7b1a7`.

`SELECT resolution_origin, COUNT(*) FROM resolved_edge GROUP BY 1` - base (ctf.db, 2095 rows):

```
corpus_unique|15
module_plane|9
```

HEAD (ctf-head.db, 2747 rows):

```
corpus_unique|15
module_plane|11
receiver|7
```

`SELECT reason, COUNT(*) FROM unresolved GROUP BY 1` - base:

```
(no rows: the kotlin arm had no drops channel)
```

HEAD:

```
ambiguous|12
inferred|5
no_corpus_def|7
```

| movement | fact |
| --- | --- |
| `receiver` 0 -> 7 | the fixtures' six receiver legs plus `Gadget.spin()` in the module_resolve corpus (was `corpus_unique` at base). |
| `corpus_unique` 15 -> 15 | `-1`: the module_resolve `Gadget.spin()` name guess moved to `receiver`; `+1` each: `makeWidget()` in returnLeg and `Widget(1)` in the receivers lib.kt (plain uppercase calls keep their name-match/module legs). |
| `module_plane` 9 -> 11 | the receivers fixture's `Widget(2)` (use.kt) and `makeWidget()` (returnLeg) same-package legs. |
| unresolved 0 -> 24 | the new kotlin drops channel: 5 `inferred` (returnLeg `w.run`, shadow `run`, sample.kt `store.get`/`xs.fold`/`xs.map` - `Named` receivers on non-corpus types `Store`/`List`), 7 `no_corpus_def` (free calls with no corpus def, e.g. `Cfg(...)`, `pick(host)`, `listOf`, `contains`), 12 `ambiguous` (free calls the corpus declares more than once, and `Named` receivers whose corpus type has no such member). |

## Tests changed

| test | old expectation | new expectation | why the old one encoded a guess |
| --- | --- | --- | --- |
| `tests/131_kotlin_module_resolve.rs` `the_corpus_name_match_stays_the_last_leg` -> `the_member_call_binds_through_the_receiver_plane` | `main`'s `spin` edge targets Gadget.kt at origin `corpus_unique` (a bare-name match over the whole corpus, unique only because the corpus declares `spin` once) | same target, origin `receiver`, test renamed; one-line lane K1 comment | The site is `Gadget.spin()`: a member call whose receiver the phase-1 receiver walk names (`Named(Gadget)`). The owner table answers it; the corpus name-match never runs for a `Named` receiver under the lane D law. |
| `tests/6_kind_vocab.rs` `wire_output_is_byte_identical_to_the_946460d75_golden` (golden `tests/fixtures/kind_vocab/wire_golden.jsonl`) | The wire stream over the 185-file corpus carried no `method_owner` records for the four kotlin corpus files (`kotlin/docs.kt`, `kotlin/sample.kt`, `df_loops/sample.kt`, `kotlin_modules/sample.kt`). | Regenerated with the current binary: exactly 5 added `method_owner` records (Car.drive + Car.name, Repo.fetch + Repo.cache, Peer), 2175246 -> 2175772 bytes; zero removed lines, zero kind changes, zero span shifts (diff checked before the copy). | Phase 1 now mints `MethodOwner` rows for kotlin class-body members, so the corpus's kotlin files contribute the same aux rows rust and go already did. The brief lists this golden as regen-only when a panic names it. |

New: `tests/137_kotlin_receiver_legs.rs` (3 tests, real `extract` binary, expectations derived from the fixture bytes, `callee_start` asserted against the hand-located def spans so `Widget.run` can never be satisfied by `Decoy.run`). New fixtures: `tests/fixtures/kotlin_receivers/{lib,use}.kt`, outside the scip ratchet corpus `tests/fixtures/kotlin/`.

## Gate

Last lines of `cargo test --features cli --no-fail-fast` at HEAD (7dc7b1a7 + the regenerated wire golden):

```
running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

| order | fact |
| --- | --- |
| targeted | 137 (3), 131 (5), 90_mutation_battery (14), golden_parity (10, incl. the three scip ratchets), 1_resolve_cli, 18/21/26/48/119/127/4/7/14 kotlin-adjacent files: green. |
| first full gate | one target failed: `6_kind_vocab` (wire golden drifted, see ## Tests changed); everything else green. Golden regenerated, diff audited as 5 additive `method_owner` records, target green in isolation. |
| final full gate | exit clean, no failed target (the run footer lists no `error:` line; the last targets and doc-tests all report ok). |

## Blocked

Empty. Nothing blocks the delivered work.
