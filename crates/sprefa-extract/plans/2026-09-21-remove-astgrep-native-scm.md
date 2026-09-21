# Remove ast-grep. One native `.scm` engine with host predicates.

Owner decision of 2026-09-21, final: ast-grep leaves this crate. One query
engine, native tree-sitter `.scm`, extended by a Rust module of host predicates
that carries over the relations the ast-grep lowering offered. Soopy stays the
edit engine.

Investigation worktree `main-codex-attribution` at `426dbc5c`, read through
`ryi` built from that checkout. This supersedes section 6 of
`plans/2026-09-21-scm-superset.md`; that plan's section 5 design is the base and
is kept.

## 0. The pinned version, and the rest of the workspace

`Cargo.toml:44-46` pins `ast-grep-core = "0.38"`, `ast-grep-language = "0.38"`,
`ast-grep-config = "0.38"`. All three resolve to **0.38.7** in the root
`Cargo.lock`. `tree-sitter-tsquery = "0.8"` (`Cargo.toml:177`) resolves to
0.8.0 and exists only to parse `.scm` text for the lowering.

`grep -rl ast_grep crates/*/Cargo.toml` returns exactly one path,
`crates/sprefa-extract/Cargo.toml`. Nothing else in the workspace depends on
ast-grep. No other crate is affected by this removal.

## 1. What ast-grep actually is in this crate

The owner's phase sketch treats ast-grep as the query engine. The inventory says
it is four engines, and only the first is the one the sketch names. Any plan
that deletes `ast-grep-core` has to answer for all four.

| # | engine | entry points | who depends on it |
| --- | --- | --- | --- |
| 1 | query: `.scm` lowered to a rule tree | `src/lang/5_scm_lower.rs`, `src/lang/1_ast_rule.rs` | `6_scm_family.rs`, `7_scm_rows.rs`, `2_source_query.rs`, `3_source_facts.rs`, `examples/scm_vs_yaml.rs` |
| 2 | parse plus CST projection | `src/lang/astgrep.rs` (`SgRoot`, `AstGrepParser`, `CstProjector`, `CallProjector`, `AstgrepSource`) | `rust.rs`, `ts.rs`, `go.rs`, `kotlin.rs`, `data/_0_source.rs`, the roster fallback, `project.rs:1797` |
| 3 | text pattern with metavariables | `astgrep.rs::query_patterns`, `AstPatternQuery` | `src/bin/ryi.rs` `--ast-pattern`, `2_source_query.rs`, `3_source_facts.rs` |
| 4 | edit bridge into soopy | `src/drain.rs`, `src/lang/fact.rs`, `src/types.rs` (`PendingReplaceDoc`, `BoundEdit`), `src/lang/prolog/_1_rehome.rs` | `1_ast_rule.rs::stage_request`, `3_region_writer.rs` |

`RyiLang` (`src/lang/extract_lang.rs`) implements ast-grep's `Language` and
`LanguageExt`. Every one of the four engines reaches a grammar through
`RyiLang::get_ts_language`, whose `Sg` arm calls
`ast_grep_language::SupportLang::get_ts_language`. That one method is the joint
the whole removal turns on.

## 2. Inventory, counted from the tree

### Files that import an ast-grep crate or `tree_sitter_tsquery`

| file | lines | imports | what it holds |
| --- | --- | --- | --- |
| `src/lang/1_ast_rule.rs` | 672 | `ast_grep_config`, `ast_grep_core` | 23 fns: `query_ast_rule`, `query_ast_rule_with_content`, `make_match`, the YAML decode path, `rule_wire`, `stage_request`, `stage_request_batch` |
| `src/lang/5_scm_lower.rs` | 647 | `tree_sitter_tsquery` | 26 fns: `lower_scm`, `scm_language`, `parse_scm`, `lower_definition`, `lower_predicate`, `lower_node`, `lower_named_node`, the argument decoders |
| `src/lang/astgrep.rs` | 581 | `ast_grep_core`, `ast_grep_language` | 22 fns across `AstGrepParser`, `CstProjector`, the call projector, `AstgrepSource`, `query_patterns`, `call_drops` |
| `src/lang/prolog/_1_rehome.rs` | 424 | `ast_grep_config`, `ast_grep_core` | the YAML-rule prolog rehome, `type Parsed = AstGrep<StrDoc<RyiLang>>` |
| `src/lang/2_source_query.rs` | 362 | `ast_grep_language::LanguageExt` | the native executor `collect_spanned_matches` and the `SourceQuery` dispatch |
| `src/lang/6_scm_family.rs` | 355 | (via `ast_rule`, `scm_lower`) | `project_kotlin_call`, `lowered_spans` |
| `src/lang/7_scm_rows.rs` | 703 | `ast_grep_core::tree_sitter::LanguageExt` | `file_captures`, `lowered_spans`, `native_captures`, `rows` |
| `src/lang/fact.rs` | 271 | `ast_grep_core::meta_var`, `Matcher`, `Node` | `FactMatcher`, the Datalog-backed matcher |
| `src/drain.rs` | 187 | `ast_grep_core` replacer, source, `Matcher`, `Node` | the `Doc` impl over `PendingReplaceDoc`, `Edit<String>` to `soopy::TextEdit` |
| `src/lang/extract_lang.rs` | 186 | `ast_grep_core`, `ast_grep_language` | `RyiLang`, its `Language` and `LanguageExt` impls |
| `src/types.rs` | 4127 | `ast_grep_core`, `ast_grep_language` | `RyiLang::from_path` shim at `:2735-2738`, `PendingReplaceDoc`, `BoundEdit` carrying `ast_grep_core::source::Edit<String>` |

Total in the eleven files: 8,515 lines, of which 1,900 (`1_ast_rule.rs` plus
`5_scm_lower.rs` plus `astgrep.rs`) are marked for deletion outright.

### Callers outside those files

| caller | what it uses |
| --- | --- |
| `src/lang/rust.rs:22,3386-3396` | `AstGrepParser`, `CstProjector` |
| `src/lang/ts.rs:25,4075-4085` | same |
| `src/lang/go.rs:29,2747-2757` | same |
| `src/lang/kotlin.rs:39,1337-1347` | same |
| `src/lang/data/_0_source.rs:9` | `AstgrepSource` for its cst plane |
| `src/lang/mod.rs:11-13,50-61,71-77,98` | module declarations and re-exports |
| `src/lang/3_source_facts.rs:13-14,240-333` | `query_ast_rule`, `query_patterns`, `AstRuleCapture`, `AstRuleMutationProposal` |
| `src/project.rs:1797` | `crate::lang::astgrep::call_drops` |
| `src/bin/ryi.rs:31-34,927-981` | `AstPatternQuery`, `query_patterns` |
| `src/3_region_writer.rs:96` | `proposal.stage_request` |
| `src/lib.rs:74-88` | the public re-export surface |
| `examples/scm_vs_yaml.rs` (102 lines) | `lower_scm`, `query_ast_rule`, `decode_ast_rule_yaml` |

### Tests that exercise them

| test | lines | tests | what it pins |
| --- | --- | --- | --- |
| `tests/30_ast_rule.rs` | 312 | 7 | every `AstRule` variant, YAML decode, `query_patterns` |
| `tests/35_extract_lang.rs` | 292 | 10 | `RyiLang` name table, `SupportLang::all_langs()` roster parity, `expando_char` parity, a YAML rule on prolog, `NoGrammar` |
| `tests/36_drain.rs` | 269 | 8 | `SupportLang::Rust.ast_grep`, `PendingReplaceDoc::open`, edit staging |
| `tests/37_fact_matcher.rs` | 243 | 7 | `FactMatcher` as an ast-grep `Matcher` |
| `tests/38_move_perf.rs` | 211 | 4 | the rehome edit path |
| `tests/144_scm_lower.rs` | 252 | 1 | the `AstRule`, `StopBy` and `ScmLowerError` variant census snapshot |
| `tests/145_query_predicate_scope.rs` | 88 | 5 | a match is gated by its own pattern's predicates only |
| `tests/150_fast_scm_kotlin.rs` | 72 | 1 | the kotlin call golden |
| `tests/157_fast_scm_rows.rs` | 207 | 5 | row field sets, the TypeSpec tables, the streamed shape |
| `tests/158_fast_scm_kotlin.rs` | 145 | 4 | pinned kotlin row counts, symbol spellings, per-file independence |
| `tests/159_fast_scm_judge.rs` | 220 | 2 | agreement with scip-typescript |
| `tests/160_fast_scm_ts.rs` | 134 | 3 | pinned ts row counts and the time budget |
| `tests/161_fast_scm_ratchet.rs` | 188 | 2 | the checked-in ts floors, scope graph against resolve |
| `tests/4_capability_parity.rs` | 512 | 2 | names `query_patterns` and an `astgrep` fixture row |

Tests 162 to 166 consume rows, not queries, and are untouched.

## 3. Grammars

`SupportLang` in ast-grep-language 0.38.7 has 23 variants. `RyiLang::Sg` wraps
all of them and `astgrep.rs:158` makes `AstgrepSource` the roster's catch-all
for every path `SupportLang::from_path` answers, so all 23 are reachable today.

Seven grammars are already direct deps of this crate and need nothing:
go, python, kotlin-sg, json, yaml, html, plus the five ast-grep never had
(prolog, md, toml-ng, gdscript, commonlisp).

Sixteen are reached **only** through `ast_grep_language::get_ts_language`. Each
needs a direct `tree-sitter-*` dep. Every one is already in the lock as an
ast-grep transitive, so every one is on crates.io at the stated version and
cargo unifies rather than duplicates. The const column is the exact symbol
`ast-grep-language-0.38.7/src/parsers.rs` uses, which is what a byte-identical
port has to name.

| `SupportLang` | crate | locked version | crates.io | const to call |
| --- | --- | --- | --- | --- |
| `Bash` | `tree-sitter-bash` | 0.25.1 | yes | `LANGUAGE` |
| `C` | `tree-sitter-c` | 0.24.2 | yes | `LANGUAGE` |
| `Cpp` | `tree-sitter-cpp` | 0.23.4 | yes | `LANGUAGE` |
| `CSharp` | `tree-sitter-c-sharp` | 0.23.5 | yes | `LANGUAGE` |
| `Css` | `tree-sitter-css` | 0.23.2 | yes | `LANGUAGE` |
| `Elixir` | `tree-sitter-elixir` | 0.3.5 | yes | `LANGUAGE` |
| `Haskell` | `tree-sitter-haskell` | 0.23.1 | yes | `LANGUAGE` |
| `Java` | `tree-sitter-java` | 0.23.5 | yes | `LANGUAGE` |
| `JavaScript` | `tree-sitter-javascript` | 0.23.1 | yes | `LANGUAGE` |
| `Lua` | `tree-sitter-lua` | 0.2.0 | yes | `LANGUAGE` |
| `Php` | `tree-sitter-php` | 0.23.11 | yes | `LANGUAGE_PHP_ONLY` |
| `Ruby` | `tree-sitter-ruby` | 0.23.1 | yes | `LANGUAGE` |
| `Rust` | `tree-sitter-rust` | 0.24.2 | yes | `LANGUAGE` |
| `Scala` | `tree-sitter-scala` | 0.24.1 | yes | `LANGUAGE` |
| `Swift` | `tree-sitter-swift` | 0.7.3 | yes | `LANGUAGE` |
| `TypeScript`, `Tsx` | `tree-sitter-typescript` | 0.23.2 | yes | `LANGUAGE_TYPESCRIPT`, `LANGUAGE_TSX` |

Sixteen crates, seventeen variants. `Php` taking `LANGUAGE_PHP_ONLY` rather than
`LANGUAGE` is the one place a careless port changes behaviour: `LANGUAGE`
accepts HTML around the PHP, `LANGUAGE_PHP_ONLY` does not.

Three tables move with the grammars, because `SupportLang` owns them and nothing
else does.

1. **Extensions**, `ast-grep-language-0.38.7/src/lib.rs:460-489`. This is what
   `SupportLang::from_path` reads, so the roster's catch-all arm needs it
   verbatim: bash `bash bats cgi command env fcgi ksh sh tmux tool zsh`,
   c `c h`, cpp `cc hpp cpp c++ hh cxx cu ino`, cs `cs`, css `css scss`,
   elixir `ex exs`, go `go`, haskell `hs`, html `html htm xhtml`, java `java`,
   javascript `cjs js mjs jsx`, json `json`, kotlin `kt ktm kts`, lua `lua`,
   php `php`, python `py py3 pyi bzl`, ruby `rb rbw gemspec`, rust `rs`,
   scala `scala sc sbt`, swift `swift`, typescript `ts cts mts`, tsx `tsx`,
   yaml `yaml yml`.
2. **Aliases**, `lib.rs:351-375`, which `RyiLang::parse_name` reaches through
   `SupportLang::from_str` and which `ryi query --lang` therefore accepts.
3. **Display**, `lib.rs:280-284`, `write!(f, "{:?}", self)`. The enum's own
   spelling is what `RyiLang::name()` serializes: `Rust`, `TypeScript`, `Tsx`,
   `CSharp`, `JavaScript`, `Cpp`, `Php`. Any golden or SQLite column carrying a
   language name carries those exact strings. The replacement enum keeps the
   variant names character for character.

The `expando_char` and `meta_var_char` tables (`lib.rs:190-225`) are pattern-side
only. They matter while engine 3 lives and die with it.

## 4. The predicate table

`lower_predicate` (`src/lang/5_scm_lower.rs:329-452`) accepts exactly the rows
below, each also with a `not-` prefix that wraps the result in `AstRule::Not`.
A label is a top-level pattern carrying a capture directly on its root
(`5_scm_lower.rs:108-120`).

`stopBy` in the lowering has three spellings, from `lower_predicate:437-447`:
absent or `"end"` gives `StopBy::End`, the string `"neighbor"` gives
`StopBy::Neighbor`, and a bare identifier gives `StopBy::Rule(Matches(label))`.
The semantics below are read off
`ast-grep-config-0.38.7/src/rule/relational_rule.rs` and `rule/stop_by.rs`.

| `.scm` | lowered to | native replacement | one-line semantics | tree-sitter API |
| --- | --- | --- | --- | --- |
| `#inside? @c L` | `Inside { stop_by: End }` | host `#inside?` | some strict ancestor of `@c` is in `L`'s node set | `Node::parent()` in a loop from `@c.parent()` to the root |
| `#inside? @c L "neighbor"` | `Inside { stop_by: Neighbor }` | same, third argument | the immediate parent is in `L` | one `Node::parent()` |
| `#inside? @c L S` | `Inside { stop_by: Rule(S) }` | same | ancestors upward, stopping at and including the first in `S`'s set | `Node::parent()` loop with an inclusive break |
| `#has? @c L` | `Has { stop_by: End }` | host `#has?` | some strict descendant is in `L`'s set | pre-order walk with `TreeCursor`, self skipped |
| `#has? @c L "neighbor"` | `Has { stop_by: Neighbor }` | same | some direct child, named or anonymous, is in `L`'s set | `TreeCursor` over all children, not `named_children` |
| `#has? @c L S` | `Has { stop_by: Rule(S) }` | same | descendants, pruning every subtree rooted at a node in `S`'s set | `TreeCursor` walk with a prune test |
| `#precedes? @c L` | `Precedes { stop_by: End }` | host `#precedes?` | some later sibling is in `L`'s set | `Node::next_sibling()` loop |
| `#precedes? @c L "neighbor"` | `Precedes { Neighbor }` | same | the immediate next sibling, named or not | one `Node::next_sibling()` |
| `#precedes? @c L S` | `Precedes { Rule(S) }` | same | later siblings, stopping at and including the first in `S`'s set | `next_sibling()` with an inclusive break |
| `#follows? @c L` | `Follows { stop_by: End }` | host `#follows?` | some earlier sibling is in `L`'s set | `Node::prev_sibling()` loop |
| `#follows? @c L "neighbor"` / `S` | `Follows { Neighbor / Rule }` | same | mirror of `#precedes?` | `prev_sibling()` |
| `#nth-child? @c "N"` | `NthChild { position }` | host `#nth-child?` | `@c`'s 1-based index among its parent's **named** children satisfies `N` | `Node::parent()`, `named_children(&mut cursor)` |
| `#nth-child? @c "An+B"` | same, functional position | same | the same index satisfies the `An+B` form | same |
| `#nth-child? @c "N" L` | `NthChild { of_rule }` | same | the index is counted only over named children in `L`'s set | same plus the set test |
| `#nth-child? @c "N" "reverse"` | `NthChild { reverse }` | same | the index is counted from the last named child | same |
| `#range? @c "l:c" "l:c"` | `Range { start, end }` | host `#range?` | `@c` starts and ends at exactly those 0-based row and column points | `Node::start_position()`, `Node::end_position()` |
| `#match? @c "re"` | `AstRule::Regex` | stays native, already `sprefa-match?` | `@c`'s text matches the regex | `general_predicates`, `regex::bytes` |
| `#not-match? @c "re"` | `Not(Regex)` | stays native, `sprefa-not-match?` | negation of the above | same |
| `#pattern? @c "snippet" [$M L]...` | `AstRule::Pattern` plus constraints | **no replacement, dropped** | see section 8 | none |
| `#eq?`, `#not-eq?`, `#any-of?`, `#not-any-of?`, `#any-eq?` | never lowered | stay native | text equality and membership over capture text | tree-sitter's own `TextPredicateCapture`, plus `sprefa-eq?` for capture-to-capture |
| `#set! key value` | never lowered, silently dropped | **kept as match metadata** | attaches a key and value to every match of the pattern | `Query::property_settings(pattern_index) -> &[QueryProperty]` |

Two counted facts about what the lowering does to each of the above, from the
probe table in `plans/2026-09-21-scm-superset.md:37-56`: the lowering emits no
captures at all (`captures []`), and a second predicate on a different capture
is refused with `FocusConflict`. Neither limit survives into the native engine,
because `QueryMatch::captures` already carries every capture with its own span.

`#set!` handling is a requirement, not a nicety. Today tree-sitter routes it to
`property_settings()`, nothing in the crate reads that, and the row comes back
unchanged, so a `.scm` vendored from Helix or Zed loses the taxonomy it carries.
The new `Match` type holds it.

### Match limit

Three spellings exist today: an `assert!` at `6_scm_family.rs:87`, a recoverable
`ScmError::MatchLimit` at `7_scm_rows.rs:456`, and no check at all in
`collect_spanned_matches`, which is the one `ryi query` uses. The new engine has
exactly one, after the cursor is dropped, returning a named error. That closes
the second acceptance line of `issues/lab-scopegraph-queries/item.md`, "a true
is a named stop, never a silent partial", and the third, "the engine uses
`matches()` or `captures()` exclusively and a test pins which": it is
`matches()`, which all three current executors already use.

## 5. The design

One module directory, following the crate's own idiom for a multi-file language
module (`src/lang/prolog/`, `src/lang/python/`, `src/lang/data/`: a `mod.rs`
plus `_N_part.rs` files, declared in `src/lang/mod.rs` without a `#[path]`).

```
src/lang/scm/
  mod.rs            run_query, Match, ScmQueryError, the label index
  _0_labels.rs      label discovery and the per-file node-id sets
  _1_predicates.rs  the seven host predicates over QueryMatch captures
```

`mod.rs` entry:

```rust
pub fn run_query(lang: RyiLang, query_text: &str, source: &[u8])
    -> Result<Vec<Match>, ScmQueryError>;

pub struct Match {
    pub pattern: u32,
    pub captures: Vec<(String, Span, String)>,
    pub set: BTreeMap<String, String>,
}
```

Six properties the entry has to hold.

1. **One parse.** `run_query` parses once and owns the tree. Neither
   `7_scm_rows.rs` nor `6_scm_family.rs` opens a second `Parser` over bytes the
   family path already parsed.
2. **Every capture.** `Match.captures` is the full `QueryMatch::captures` list,
   one entry per capture with its own span and text, repeated names preserved in
   order. `query_tree_sitter`'s `BTreeMap` projection
   (`2_source_query.rs:112-130`) stays only as the `ryi query` display shape, and
   the quantifier collapse it causes gets a test that names it.
3. **`#set!` kept.** `Match.set` is `Query::property_settings(pattern_index)`
   folded into a map, key to value, absent value spelled as the empty string.
4. **Labels resolved once per file.** A top-level pattern carrying a capture on
   its root is a label. It contributes no rows. Each label is compiled as its own
   `tree_sitter::Query` and run once over the whole tree; the node ids of its root
   capture become a `HashSet<usize>`. A host predicate is then an ancestor,
   sibling or descendant walk with a set membership test: O(file) once per label,
   O(depth) or O(siblings) per test. Running a sub-query per candidate node
   instead would be O(file) per test and is refused.
5. **Host predicates through `general_predicates()`.** `_1_predicates.rs`
   dispatches the seven operators of section 4 plus their `not-` forms. An
   operator outside the table is a named error, not a silent pass. This replaces
   `validate_predicates` (`2_source_query.rs:167-182`), which today names only
   three operators and lets `#not-eq?`, `#any-of?` and `#any-eq?` past unchecked
   to the binding's own evaluator. The new validator names the whole native
   family explicitly so acceptance is stated rather than inherited.
6. **One match-limit check.** After `drop(matches)`, one
   `cursor.did_exceed_match_limit()`, one `ScmQueryError::MatchLimit { path }`.

`RyiLang` becomes a plain enum owned by the roster. Its `Sg(SupportLang)` arm
expands into the seventeen variants of section 3 under their current spellings,
so `name()`, `parse_name()` and every serialized language string are unchanged.
The `Language` and `LanguageExt` impls go when engine 3 goes; `get_ts_language`
becomes an inherent method returning `tree_sitter::Language`.

Deleted at the end: `lowered_spans` in both consumers
(`7_scm_rows.rs:371-401`, 31 lines; `6_scm_family.rs:123-143`, 21 lines), the
private `Parser` in `native_captures` (`7_scm_rows.rs:413-425`, 13 lines), the
span-intersection gate (`:446-453`, 8 lines), `5_scm_lower.rs` (647),
`1_ast_rule.rs` (672), `astgrep.rs` (581), and the four deps.

## 6. Phases

Eight, not six. The owner's six are phases 1 to 4 and 7 to 8 below. Phases 5 and
6 exist because engines 2 and 4 of section 1 stand between phase 4 and the
deletion in phase 7: `astgrep.rs` cannot be deleted while it is the parser for
rust, ts, go, kotlin and the roster fallback, and `ast-grep-core` cannot leave
`Cargo.toml` while `drain.rs` implements its `Doc` trait.

Every command under `timeout 10`, cargo under `timeout 600`, with
`CARGO_TARGET_DIR=$PWD/target`, run from `crates/sprefa-extract`.

Baseline to record before phase 1, and to compare against at phase 8:

```
timeout 600 cargo test --features cli --no-fail-fast 2>&1 | grep -E '^test result:'
timeout 600 cargo build --features cli --bin ryi --timings
```

The last recorded counts are 1005 passed across 190 binaries
(`TASKS/lane-fast-tier-on-scm.BRIEF.md:68`). Two `golden_parity` root-prefix
diffs are pre-existing and are not this lane's.

### Phase 1: the grammar roster

| | |
| --- | --- |
| owns | `Cargo.toml`, `Cargo.lock`, `src/lang/extract_lang.rs`, `tests/35_extract_lang.rs` |
| forbids | `src/lang/5_scm_lower.rs`, `src/lang/1_ast_rule.rs`, `src/lang/astgrep.rs`, `src/lang/7_scm_rows.rs`, `src/lang/6_scm_family.rs`, `src/drain.rs`, `src/lang/fact.rs` |

Add the sixteen `tree-sitter-*` deps of section 3 with the locked versions. Turn
`RyiLang::Sg(SupportLang)` into the seventeen variants, spelled exactly as the
`SupportLang` variants are. Copy the extension table, the alias table and the
`LANGUAGE` const per variant. `get_ts_language` stops calling
`ast_grep_language`. `RyiLang` keeps its `Language` and `LanguageExt` impls,
with the `expando_char` and `meta_var_char` tables copied from
`ast-grep-language-0.38.7/src/lib.rs:190-225`, because engines 2, 3 and 4 still
need them.

Counted checks:

- `cargo tree -e normal -p ast-grep-language` no longer lists this crate as a
  dependent of it for grammars; `ast-grep-language` is still a dep of the crate
  for the trait only.
- `cargo tree -d` lists no duplicate `tree-sitter` core and no duplicate grammar.
- `cargo test --test 35_extract_lang` passes its 10 tests, with the
  `SupportLang::all_langs()` roster-parity test rewritten against the new enum
  and the `expando_char` parity test rewritten as a literal table.
- Full suite equals the baseline count.

Risk: the `Php` const. Taking `LANGUAGE` instead of `LANGUAGE_PHP_ONLY` changes
which bytes parse and silently changes every `.php` CST row. The check is one
fixture with `<?php` framing, parsed both ways, row counts compared.

Second risk: the Display spelling. If a variant is renamed to a Rust-idiomatic
spelling, every golden and every `_input_path` language column drifts. The check
is `git diff --stat tests/fixtures` being empty.

### Phase 2: `src/lang/scm/`

| | |
| --- | --- |
| owns | `src/lang/scm/**`, `src/lang/mod.rs` (declaration only), `tests/167_scm_engine.rs` |
| forbids | everything in phase 1's owns, plus `2_source_query.rs`, `7_scm_rows.rs`, `6_scm_family.rs` |

Build `run_query`, the label index, the seven host predicates with all three
`stopBy` spellings and the `not-` prefixes, `#set!` capture, and the one
`did_exceed_match_limit` check. Nothing calls it yet.

Counted checks, all on one committed rust fixture, each a counted match number:

- one test per predicate per `stopBy` spelling: 7 predicates, 3 spellings where
  the predicate takes one, plus the `not-` form of each. Every assertion is a
  count, not a boolean.
- the complement identity over `src/project.rs`, reproducing
  `TASKS/lane-scm-relations-doc.RESULT.md:48-56` through `run_query`:
  `((call_expression) @m (#match? @m ""))` gives 1121, `(#inside? @m closure)`
  gives 282, `(#not-inside? @m closure)` gives 839, the two sum to 1121, and
  `(#inside? @m closure "neighbor")` gives 70.
- `(#set! role def)` on a one-capture pattern returns a match whose `set` is
  `{"role": "def"}`.
- a query with 1 pattern and a deliberately explosive match set returns
  `MatchLimit` rather than a truncated row list.

Risk, and it is the one that costs a silent row-count change rather than a build
error: **`stopBy: end` against `neighbor` against a rule.** Four ways to get it
wrong, each producing plausible-looking output.

1. `end` on `#inside?` walks `Node::parent()` starting at the parent, never at
   `@c` itself. Including self turns every `(#inside? @m closure)` on a
   `closure_expression` capture into a match and moves the 282 up.
2. `neighbor` on `#has?` walks **all** children, not `named_children`.
   `ast-grep-core-0.38.7/src/tree_sitter/mod.rs:148-155` builds its child
   iterator from `goto_first_child` and `child_count`, so anonymous nodes are in
   it. Using `named_children` for `#has? "neighbor"` drops every match whose
   target is an anonymous token.
3. `neighbor` on `#precedes?` and `#follows?` is `next_sibling` and
   `prev_sibling`, not `next_named_sibling`. Same trap, opposite family.
4. `#nth-child?` is the one place named-only **is** correct:
   `ast-grep-config-0.38.7/src/rule/nth_child.rs:211-218` filters
   `parent.children()` by `is_named()` before indexing. A port that reuses the
   `#has?` child iterator here shifts every ordinal.

The `stopBy: Rule` forms differ again by family. On `#inside?`, `#precedes?` and
`#follows?` the stop is inclusive: the ancestor or sibling that matches the stop
rule is itself tested before the walk ends (`stop_by.rs:149-159`,
`inclusive_until`). On `#has?` the stop is a prune: a child that matches the stop
rule is not descended into, but the child itself is still tested
(`relational_rule.rs:162-173`). Implementing the prune as an inclusive
take-while, or the inclusive walk as a prune, changes counts on every query that
uses a third argument.

### Phase 3: `ryi query` on the new engine

| | |
| --- | --- |
| owns | `src/lang/2_source_query.rs`, `src/0_query.rs`, `tests/145_query_predicate_scope.rs` |
| forbids | `src/lang/scm/**` beyond additive re-export, `7_scm_rows.rs`, `6_scm_family.rs`, `astgrep.rs` |

`query_tree_sitter_spans` becomes a projection over `run_query`.
`collect_spanned_matches`, `matches_predicates`, `predicate_matches`,
`validate_predicates` and `rewrite_predicates` move into `src/lang/scm/` or are
deleted where `run_query` subsumes them. `query_language` reaches the roster
through `RyiLang::parse_name` exactly as it does today. The `BTreeMap`
projection in `query_tree_sitter` (`:112-130`) stays as the display shape.

Counted checks:

- `cargo test --test 145_query_predicate_scope`: 5 passed, 0 failed, the tests
  unchanged in content. Their rule, a match is gated by its own pattern's
  predicates only, is the new engine's rule.
- the ten probe queries of `plans/2026-09-21-scm-superset.md:153-161` return
  their stated row counts through `ryi query`, with one change: `(#inside? @v fn)`
  no longer reports `predicate #inside? is not allowed` but evaluates.
- `(#set! role def)` through `ryi query` emits the setting in the row rather than
  dropping it silently.
- a new test names the quantifier collapse: `(block (_)+ @s)` returns N spanned
  captures from `query_tree_sitter_spans` and one map key from
  `query_tree_sitter`.

Risk: `rewrite_predicates` renames `#match?`, `#not-match?` and `#eq?` into
`#sprefa-*` so they reach `general_predicates()` rather than the binding's own
evaluator. `#not-eq?` and `#any-of?` are deliberately left alone and are
evaluated by tree-sitter at `tree-sitter-0.25.10/binding_rust/lib.rs:3450` and
`:3494`. If the new validator renames those too without also implementing them,
they stop filtering and every query using them widens. The check is the four
rows of the superset plan's measured table: `#not-eq? @v "nope"` gives 1,
`#not-eq? @v "needle"` gives 0, `#any-of? @v "needle" "x"` gives 1,
`#any-of? @v "zzz"` gives 0.

### Phase 4: the fast families

| | |
| --- | --- |
| owns | `src/lang/7_scm_rows.rs`, `src/lang/6_scm_family.rs` |
| forbids | `5_scm_lower.rs`, `1_ast_rule.rs`, `astgrep.rs`, `2_source_query.rs`, `Cargo.toml` |

Both call `run_query`. `lowered_spans` goes from both. `native_captures` loses
its private `Parser` and its `BTreeSet<Capture>` return, becoming a projection
over grouped matches, so `rows` (`7_scm_rows.rs:466-539`) stops rebuilding the
capture association by span-containment arithmetic. The `assert!` at
`6_scm_family.rs:87` becomes the shared recoverable error.

Counted checks:

- `tests/150`, `157`, `158`, `159`, `160`, `161`: identical row counts, identical
  ratchet floors, identical golden bytes. The three bundled `.scm` files carry
  zero predicates (`grep -n '(#' queries/*/scip.scm` is empty across
  `queries/rust/scip.scm` 82 lines, `queries/typescript/scip.scm` 66,
  `queries/kotlin/scip.scm` 103), so this phase is byte-identical by
  construction. Any drift is a defect in the port.
- one parse per file, confirmed by trace span count on a single-file run.
- `tests/160`'s time budget still holds; removing a second parse should lower it.

Risk: the six `SPAN_LABELS` (`7_scm_rows.rs:18-25`) were the L1 selection gate.
With `lowered_spans` gone, every native match reaches `rows`, including matches
the L1 rule silently dropped. Because no bundled query carries a predicate, the
L1 set and the native set are the same set today. The check that proves it: run
phase 3's binary against phase 4's on the kotlin and ts fixture corpora and diff
the JSONL byte for byte before deleting `lowered_spans`.

### Phase 5: the parser and the CST projection

| | |
| --- | --- |
| owns | a new `src/lang/parse.rs` (or `src/lang/9_parse.rs` under the numeric idiom), `src/lang/rust.rs`, `src/lang/ts.rs`, `src/lang/go.rs`, `src/lang/kotlin.rs`, `src/lang/data/_0_source.rs`, `src/project.rs` (the `call_drops` wire only), `src/lang/mod.rs` |
| forbids | `src/lang/scm/**`, `7_scm_rows.rs`, `6_scm_family.rs`, `2_source_query.rs`, `drain.rs`, `fact.rs` |

This is the phase the owner's sketch does not have and the tree requires.
`astgrep.rs` is not only the lowering's runner: it is `AstGrepParser` (the
`Parser` seam impl), `CstProjector` (the `CstF` walk), a `CallF` projector,
`AstgrepSource` (the roster's catch-all `Source`), `call_drops` (wired at
`project.rs:1797`) and `query_patterns` (engine 3, reached from
`src/bin/ryi.rs:978-981`).

Port `SgRoot` to an owned `tree_sitter::Tree` plus its source `String`. Port the
`CstF` walk: it is already described in the module header as an iterative
pre-order DFS over named nodes only, with unnamed nodes reparenting their named
descendants to the nearest named ancestor, which is a `TreeCursor` walk with no
ast-grep in it. Keep `AstgrepSource`'s roster position and rename it; its
`matches` becomes the extension table of section 3 rather than
`SupportLang::from_path`.

Decide `query_patterns` here, not later. It is engine 3 and it has no native
replacement: a text pattern with `$META` variables is not a tree-sitter query.
The recommendation is to remove `--ast-pattern` and `AstPatternQuery` with it,
because `.scm` with captures now answers the same question with per-capture
spans, and because keeping it is the only thing that keeps `ast-grep-core` in
`Cargo.toml` after phase 6. If the owner keeps it, the removal stops here and
section 8's first blocker stands.

Counted checks:

- every `cst` golden under `tests/fixtures` is byte-identical. This is the
  largest single risk surface in the lane and the only check that matters.
- `tests/4_capability_parity.rs` 2 passed, with its `query_patterns` row and its
  `astgrep` fixture row rewritten to the new names.
- `cargo test --features cli --no-fail-fast` equals the baseline count.

Risk: the CST walk's treatment of anonymous nodes. `ast_grep_core::Node::children`
yields every child; the projector then filters by `is_named`. A
`TreeCursor`-based port that uses `goto_first_child` plus
`goto_next_sibling` reproduces it; one that uses `named_child(i)` skips the
reparenting step and loses every named node under an anonymous one.

### Phase 6: the edit bridge

| | |
| --- | --- |
| owns | `src/drain.rs`, `src/lang/fact.rs`, `src/types.rs` (the ast-grep-typed fields only), `src/lang/prolog/_1_rehome.rs`, `src/lang/4_owned_region.rs`, `src/3_region_writer.rs`, `tests/36_drain.rs`, `tests/37_fact_matcher.rs`, `tests/38_move_perf.rs` |
| forbids | `src/lang/scm/**`, the fast families, the parser |

`drain.rs` (187 lines) implements `ast_grep_core::source::Doc` over
`PendingReplaceDoc` so that ast-grep's `Replacer` can emit edits that
`From<BoundEdit> for soopy::TextEdit` (`drain.rs:17-30`) folds into the one
Replace soopy takes per file. Soopy is already the terminus. `ast_grep_core` is
only the intermediate edit type: `BoundEdit.edit` is
`ast_grep_core::source::Edit<String>` (`types.rs:3973`), three byte fields and a
string.

Replace `Edit<String>` with a crate-owned struct carrying `position`,
`deleted_length` and `inserted_text`, drop the `Doc` impl, and keep
`PendingReplaceDoc` holding its `tree_sitter::Tree` directly rather than through
the trait. `fact.rs`'s `FactMatcher` (271 lines) is an `ast_grep_core::Matcher`
impl over a Datalog fact set; it is reached only by
`1_ast_rule.rs::stage_request`, which is deleted in phase 7, so `FactMatcher`
either becomes a plain predicate over `tree_sitter::Node` or is deleted with its
only caller. `prolog/_1_rehome.rs` (424 lines) is the one rehome that drives off
`ast_grep_config::from_yaml_string` and `RuleConfig`; the other three
(`rust_rehome`, `ts_rehome`, `kotlin_rehome`) never import ast-grep, so the
prolog arm is ported onto whichever shape those three already use.

Counted checks:

- `tests/36_drain.rs` 8 passed, `tests/37_fact_matcher.rs` 7 passed,
  `tests/38_move_perf.rs` 4 passed, each rewritten against the new edit type and
  each asserting the same byte offsets it asserts today.
- `ryi move` on the prolog fixture produces the identical staged diff.
- `grep -rn 'ast_grep' src/ | wc -l` is down to the three files phase 7 deletes.

Risk: `Edit<String>` carries `Underlying = u8` (`drain.rs:14-15`), so position
and deleted_length are byte offsets needing no re-encoding. A replacement typed
in `char` offsets or in `u32` where the source exceeds 4 GiB silently corrupts
multi-byte edits. The check is a fixture with non-ASCII identifiers whose staged
edit spans one.

### Phase 7: the deletion and the doc

| | |
| --- | --- |
| owns | `src/lang/5_scm_lower.rs`, `src/lang/1_ast_rule.rs`, `src/lang/astgrep.rs`, `src/lang/mod.rs`, `src/lib.rs`, `Cargo.toml`, `Cargo.lock`, `examples/scm_vs_yaml.rs`, `tests/144_scm_lower.rs`, `tests/30_ast_rule.rs`, `docs/` |
| forbids | every `src/lang/*.rs` not listed |

Delete `5_scm_lower.rs` (647), `1_ast_rule.rs` (672), `astgrep.rs` (581),
`examples/scm_vs_yaml.rs` (102), `tests/144_scm_lower.rs` (252) and
`tests/30_ast_rule.rs` (312). Remove `ast-grep-core`, `ast-grep-language`,
`ast-grep-config` and `tree-sitter-tsquery` from `Cargo.toml`. Remove the
`AstRule`, `StopBy`, `NamedAstRule`, `AstRuleRequest`, `AstRuleMatch`,
`AstRuleCapture`, `AstRuleError`, `AstRuleMutationProposal`, `AstPatternQuery`,
`AstCaptureFact`, `SgRoot` and `AstgrepSource` names from `src/lib.rs` and
`src/lang/mod.rs`, and the `SourceQuery::AstRule` and
`SourceQuery::AstPatterns` arms from `2_source_query.rs` and
`3_source_facts.rs`.

Rewrite `docs/2_scm-with-ast-grep-relations-20260920.md` (495 lines) as
`docs/3_scm-predicates-20260921.md` under `docs/AGENTS.md`. That file forbids
source cites, type and fn names, a cite gate, the receipt block, em dashes and
the five banned words it lists in its own "Forbidden in a doc" section; it
requires every concept to carry contrast, source in, query, and literal output
produced by running the command, and reference tables last. The content changes:
the "accepted but not enforced" table empties, because fields, quantifiers,
supertypes and the wildcard all hold natively; the `FocusConflict` row goes;
"which node is reported" becomes "every capture is reported"; `#set!` gets a
section; `#eq?`, `#not-eq?` and `#any-of?` get rows they never had; `#pattern?`
gets a removal note.

Counted checks:

- `grep -rn 'ast_grep\|ast-grep\|tsquery' src/ tests/ examples/ Cargo.toml`
  returns nothing.
- `cargo tree -e normal | grep -c ast-grep` is 0.
- `cargo test --features cli --no-fail-fast` summed equals the baseline minus
  exactly the 8 tests of the two deleted test files, with no other delta.
- `docs/AGENTS.md`'s forbidden-word grep over the new page returns nothing.

Risk: the public re-export surface in `src/lib.rs:74-88`. It is a library API; a
downstream consumer inside the workspace that names `AstRule` breaks at compile
time, which is the safe failure. The check is `cargo check --workspace`.

### Phase 8: the gate and the table

| | |
| --- | --- |
| owns | nothing under `src/` |
| forbids | every source file |

Run the gate. Produce the table the owner asked for:

| measure | before | after |
| --- | --- | --- |
| lines deleted | | |
| lines added | | |
| direct deps removed | 4 | |
| transitive crates removed | | |
| cold build time, `cargo build --features cli --bin ryi` | | |
| test count | 1005 / 190 binaries | |

Transitive crates to expect gone, from `cargo tree -e normal -p ast-grep-config`:
`ast-grep-core`, `ast-grep-config`, `ast-grep-language`, `tree-sitter-tsquery`,
`bit-set`, `bit-vec`, `schemars`, `schemars_derive`, `serde_derive_internals`,
`unsafe-libyaml`. `ignore`, `globset`, `serde_yaml`, `regex` and `thiserror`
stay, because this crate already names them itself (`Cargo.toml:40,182,194`).
Sixteen grammar crates move from transitive to direct, which is a net zero on
crate count and a net zero on build time.

## 7. Where the lane is blocked

Nothing is blocked by a missing grammar. Every one of the sixteen is on crates.io
at the version already in the lock. Three things are blocked on a decision.

1. **`#pattern?` has no native replacement.** It compiles a text snippet with
   `$META` variables and matches it structurally, which is ast-grep's whole
   reason to exist and which tree-sitter has no operator for. With
   `ast_grep_core` gone there is nothing to bridge to. Reimplementing it means
   writing a pattern matcher in this crate, which every brief in this area
   forbade (`TASKS/lane-scm-lower-astgrep.BRIEF.md:262-264`). It costs nothing to
   drop: no bundled `.scm` uses it, and its only mentions are the guide and
   `tests/144`. **Recommendation: remove `#pattern?`, and with it the
   `constraints` and metavariable-binding argument form.**

2. **`query_patterns` / `--ast-pattern` is engine 3 and has no replacement
   either.** It is the same problem as `#pattern?` at CLI scale, reached from
   `src/bin/ryi.rs:927-981` and re-exported from `src/lib.rs:80`. Keeping it
   keeps `ast-grep-core` in `Cargo.toml` and the removal cannot complete.
   **Recommendation: remove the verb.** `.scm` with captures now returns
   per-capture spans, which is what the verb's `AstCaptureFact` shape carries.

3. **The YAML rule surface goes with `AstRule`.** `decode_ast_rule_yaml`,
   `AstRuleRequest`, `fix`, `AstRuleMutationProposal` and
   `SourceQuery::AstRule` are the ast-grep rule language as data, used by
   `3_source_facts.rs:276-333` and by `prolog/_1_rehome.rs`. Native `.scm`
   answers the matching half and not the rewrite half: a query does not produce
   text. The rewrite half belongs to soopy, which is already where every edit
   lands (`drain.rs:17-30`). **Recommendation: the rewrite path is expressed as
   a `.scm` that names the spans plus a soopy Replace, and `fix` as an ast-grep
   concept is removed.** If the owner wants a declarative fix surface kept, that
   is a separate lane and phase 6 stops at the edit type.

No caller was found that could not be rerouted. `project.rs:1797`'s
`call_drops`, the four `AstGrepParser` call sites, `data/_0_source.rs`'s cst
delegation and `3_region_writer.rs:96`'s `stage_request` all have a named
destination in phases 5 and 6.

## 8. Order, and what each phase buys

Phases 1 to 4 are the owner's spine and are independently green: after phase 4
the `.scm` surface is native end to end and the ast-grep deps are still present
but reached only by engines 2, 3 and 4. Phases 5 and 6 are the price of the
deletion and carry the golden risk. Phase 7 is mechanical once 5 and 6 land.
A lane that stops after phase 4 has a working superset and no removal; a lane
that starts at phase 7 does not compile.
