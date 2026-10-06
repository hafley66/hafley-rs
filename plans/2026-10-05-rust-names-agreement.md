# Rust Names agreement

The coordinator authorized one corrected run per resolver after the first comparison.
Old CLI: bb0fadcc. Provider: step 4 plus RA associated-item lookup and declaration
use offsets. Both used `--resolve --arms call,type --root <worktree> --lines -`.
The same 443 workspace source and build-script files were supplied; SHA256 content
ids were checked before and after both runs. No sources changed between runs.
No sysroot, dependency sources, compiler, SCIP or Types oracle was loaded.

Normalization compares call destinations, named type destinations, written import
routes and module declaration routes. Provenance, hop counts, duplicate rows and
null type destinations are excluded. Module locals are normalized to declared names.
POSIX `sort -u` and `comm -23/-13` produced the differences. Module routes are wire
rows, not target-specific ModulePlace records; the fixture covers shared target places.

| Family | Equal | Old only | Provider only |
| --- | ---: | ---: | ---: |
| Calls | 16,561 | 5,981 | 633 |
| Types | 10,538 | 1,748 | 213 |
| Imports | 3,431 | 302 | 3,549 |
| Written module routes | 394 | 143 | 0 |

| Old-only call category | Rows |
| --- | ---: |
| needs_types | 4,522 |
| assoc | 666 |
| unresolved | 591 |
| external | 54 |
| changed | 148 |

Categories apply in this order: a resolved provider edge at the same source offset
is changed; exact unresolved wire reasons identify needs_types and external; remaining
qualified spellings with an uppercase head or Self are assoc; remaining rows are
unresolved. Thus assoc counts spellings, including std and dependency paths, rather
than proving 666 missing workspace impl functions. For example String::new and
Vec::new have no loaded std source; sprefa's ScipMode::from_flags and RyiLang::from_path
are hafley_scm re-exports outside sprefa's owning Cargo workspace. The legacy ambiguous
wire reason also includes unresolved paths and missing corpus joins.

Associated paths use RA indexed inherent items and in-scope trait path candidates
on declared type heads, without caller-body inference. Constructors, aliases,
trait functions, private Self::helper and enum variants are fixture-covered.
Triage after the corrected run added RA enum-variant lookup for Self::Variant;
that fixture first failed and then passed. Counts above remain the frozen run,
without an additional corpus run.

Every difference is listed locally with file:line in
`crates/sprefa-extract/bench/rust-resolution-agreement/disagreements.tsv` (12,569 rows).
The TSV is gitignored and removed from tracked plans. Its verdict column distinguishes
contract-decided cases from cases requiring an independent oracle. Receiver calls
require Types under the brief; external sources abstain. Referenced workspace
dependency roots are now retained as RA-derived module relations. Remaining changed/unresolved destinations are explicitly marked for review;
this comparison does not establish which destination is correct without an oracle.
Import rows without source spans anchor line 1. No third corpus run was performed.


Post-run regression repairs retain this frozen measurement. They remove invented
star-import rows, restore referenced workspace dependency root routes, restrict
Type joins to Type facets, preserve import hop provenance, and resolve workspace
trait overrides through indexed RA lookup. No additional agreement run was made.
The receiver-call expectation changes were held until the 2026-10-06 user decision.

The 2026-10-06 user decision selects default method abstention. The reviewed
contracts and per-test list are in [failure triage](2026-10-05-rust-names-failure-triage.md).
