# Rules of hooks: pre-ryiii, revised 2026-10-05

All 14 missing-owner rows are resolved. Callback false positives fell from 20
to 0; early-return false positives fell from 7 to 0. The existing scoring query
ran once after the three fixes over all 114 non-Flow fixtures.

| rule | case_count | true_positive | false_positive | false_negative |
| --- | ---: | ---: | ---: | ---: |
| async | 114 | 0 | 0 | 6 |
| caller | 114 | 28 | 0 | 3 |
| class | 114 | 0 | 0 | 10 |
| conditional | 114 | 14 | 0 | 2 |
| early_return | 114 | 3 | 0 | 0 |
| effect_event | 114 | 0 | 0 | 12 |
| loop | 114 | 15 | 0 | 0 |
| nested_callback | 114 | 4 | 0 | 0 |
| try_catch | 114 | 0 | 0 | 2 |

The table records 64 true positives, 0 false positives, and 35 false negatives.
[Raw score](9_SCORE.json) is the unchanged output of `8_score.sql`. The upstream
suite has 118 cases, with four Flow cases retained and excluded. All 118 metadata
rows and the 99 non-Flow expected messages are in the query database. Reference:
React commit `ae74234eae6ebd62f19190731278e20bc1c37d51`.

Counts match messages as a multiset by `(fixture path, rule, hook name)`, with
actual call spans deduplicated. `case_count` is the 114-case universe for every
rule. Upstream messages have no expected source spans, so matching counts do not
prove exact call-site identity. The `pre-ryiii` SQL comparison was explicitly
authorized; registration in `ryiii` remains pending its implementation.

## Changes

- `a4916801`: TS DF traversal visits nested declarations, labeled and try bodies,
  destructuring defaults, class expressions and fields, exported variables, and
  default-export expressions through shared lifts. Existing call-result rows
  already had owners; skipped branches caused the 14 gaps.
- `266ff281`: SQL derives explicit and inferred function names from byte-safe
  CST source captures, recognizes memo/forwardRef render arguments, and requires
  React ancestry for anonymous callback findings. Namespace filtering and class
  frame exclusion remove the related caller and conditional false positives.
- `61a11247`: direct returns suppress unreachable calls in a containing block;
  loop findings take precedence over return-order findings. Guarded returns and
  returns in nested functions retain their separate owning frames.

Framework conventions remain in the query and SQL. Rust changes extract generic
syntax facts; Rust resolver files are unchanged. The benchmark uses the fast tier
and enables no checker.

## Every disagreement and remaining limits

There are 31 unequal `(path, rule, hook)` entries across 29 files, all false
negatives. Every entry has a file and reason in the
[disagreement grid](11_DISAGREEMENTS.html) and [complete JSON](10_DISAGREEMENTS.json).
[Gap output](12_GAPS.json) is `[]`; no missing owner, missing site, or parse-error
row remains. The obsolete nonempty gap grid was removed.

The remaining reference messages cover:

- 30 diagnostics for async, class, try/catch `use`, and effect-event behaviors
  outside this query's implemented rule set.
- Three bare `use` caller diagnostics, excluded by the brief's `^use[A-Z0-9]`.
- `fixtures/invalid/028_Case.tsx`: a labeled break can skip a hook that is not
  lexically inside the condition.
- `fixtures/invalid/034_Case.tsx`: a catch path can bypass a hook in a try body.

Both final control-flow cases now have DF owners. The conditional predicate
models branch ancestry, rather than a complete control-flow graph. Return
reachability likewise covers a direct return in a containing block.

## Build and validation

Built this worktree with
`KACHE_DISABLED=1 cargo build --features cli,ts-checker --bin ryii` in 46.34 seconds. Both extraction commands used that binary over the
same 114 paths: 3,038 fact rows, 13 loop rows, 20 nest rows, 8,802 query-store rows,
182 captures, 64 violations, and 0 gaps. No installed binary was used.

The owner fixture failed first with zero call-result rows. Its authored lexical
owner expectation and whole-output snapshot now pass. Callback and return tests
are table-driven over fixture directories and snapshot every SQL result column.

The full `cargo test --features cli --no-fail-fast` gate ran once: integration
results were 1,267 passed, 6 failed, and 19 ignored. Five failures reflected the
added DF rows in legacy comparisons. Those comparisons now project the recorded
V5 node spans or legacy JSON rows, preserving every legacy value and wire byte;
added TS rows are reported separately. Other languages retain their original
comparisons. No captured oracle was rewritten. The final combined targeted gate
passes all 12 tests, including the wire comparison preserving all 1,270,869
legacy bytes and positive render-wrapper findings after a UTF-8 prefix.

The remaining failure is the unchanged Rust-only test
`t_166_cleave_rust::an_unloadable_manifest_stops_the_rust_cleave_with_the_reason`.
Its actual diagnostic omits `ROOT/Cargo.toml:` from the expected prefix. It is
classified as base/unrelated to the TS and SQL changes; Rust resolver files were
left unchanged. The full gate was not repeated after the projection changes.

[Validation receipt](14_VALIDATION.json) lists every full-gate failure and its
final disposition. [Run provenance](13_RUN.json) records the binary artifact,
SHA-256, source commits, program hashes, commands, and counts. Reproduction is in
[the README](5_README.md). No merge or push was performed.
