# Rules of hooks: pre-ryiii parity, 2026-10-05

The 35 remaining false negatives are resolved. One execution of `8_score.sql`
over all 114 non-Flow fixtures reports 99 true positives, 0 false positives, and
0 false negatives. Framework rules remain in the query and bench SQL.

| rule | case_count | true_positive | false_positive | false_negative |
| --- | ---: | ---: | ---: | ---: |
| async | 114 | 6 | 0 | 0 |
| caller | 114 | 31 | 0 | 0 |
| class | 114 | 10 | 0 | 0 |
| conditional | 114 | 16 | 0 | 0 |
| early_return | 114 | 3 | 0 | 0 |
| effect_event | 114 | 12 | 0 | 0 |
| loop | 114 | 15 | 0 | 0 |
| nested_callback | 114 | 4 | 0 | 0 |
| try_catch | 114 | 2 | 0 | 0 |

[Raw score](9_SCORE.json) is the direct output of the existing scoring query.
All 118 original case metadata rows and 99 non-Flow expected messages are in the
same query database. Four Flow cases remain excluded. Reference: React commit
`ae74234eae6ebd62f19190731278e20bc1c37d51`.

## Changes

- `8f63b2a6`: generic TS DF nodes expose `is_async` and `owner_kind`, including
  `class_method`, through the same JSONL and generated SQLite column names.
  The shared owner assignment preserves innermost callable properties. Nested
  functions use their own flags; deferred JSX functions are synchronous.
- `43ac07a3`: bench SQL derives class and async findings from generic owner facts
  and CST class-field ownership. Whole-output fixtures include render wrappers
  and synchronous functions nested inside async functions.
- SQL-only edge predicates recognize bare `use`, preserve its conditional/loop/
  callback exceptions, reject its try/catch contexts, identify the labeled-break
  and try/catch conditional cases, and validate effect-event references. Lexical
  scopes exclude shadowed bindings; effect contexts consume original
  `additionalEffectHooks` regex settings through the existing `regexp()` runtime.
  The single-case adapter supplies settings without expected-message data.

Rust changes extract generic properties. Rust resolver files remain unchanged.
The query still consists of one scm++ file and one violation SQL file. No rule
implementation, new scoring script, install, merge, or push was added.

## Disagreements and scope

There are 0 disagreement entries and 0 affected files.
[Complete disagreement JSON](10_DISAGREEMENTS.json) and [gap JSON](12_GAPS.json)
are both `[]`; the [disagreement page](11_DISAGREEMENTS.html) records the empty result.
Missing owner metadata produces explicit gap rows.

Counts match messages as a multiset by `(fixture path, rule, hook name)`, with
actual spans deduplicated. `case_count` is the 114-case universe for every rule.
Expected message declarations lack source spans, so these counts do not prove
exact diagnostic-site identity. This is parity on the pinned non-Flow suite;
the CST predicates do not implement a complete CFG or JavaScript binding engine.
The explicitly authorized comparison remains `pre-ryiii` while that runner is
unavailable.

## Build and validation

The explicit worktree build
`KACHE_DISABLED=1 cargo build --features cli,ts-checker --bin ryii` completed in
105 seconds. Both extraction commands used that binary, with no checker enabled:
3,038 fact rows, 13 loop rows, 20 nest rows, 8,882 query-store rows, 198 captures,
99 violations, and 0 gaps. The installed binary was not used.

The table-driven owner fixture failed first because both requested columns were
absent on all 12 authored calls. It now compares every DF node column across
JSONL and SQLite and snapshots the complete node output. The 10 owner and legacy
targeted gates pass, preserving 1,270,869 recorded wire bytes and existing V5
oracles while projecting only the two declared new columns.

The full `cargo test --features cli --no-fail-fast` gate ran once: 1,283 integration
tests passed, 0 failed, and 19 were ignored; seven library tests and 18 binary
tests passed. The final five targeted owner/hook tests pass after the test
harness reuses the existing regexp module, removing its duplicate import.
Schema generation reports all nine artifacts current.

[Validation receipt](14_VALIDATION.json) records commands, counts, and failure
classification. [Run provenance](13_RUN.json) records the exact binary artifact,
SHA-256, source hashes, program hashes, commands, and single scoring execution.
Reproduction is in [the README](5_README.md).
