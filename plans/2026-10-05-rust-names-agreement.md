# Rust Names agreement, single run per implementation

Old CLI: build bb0fadcc. Provider: step 4 working tree. Both commands used
`--resolve --arms call,type --root <worktree> --lines -`, with the same path list
from Cargo workspace source directories and build scripts. No compiler, SCIP,
sysroot, dependency-source load, or second corpus run.

443 files were supplied. `rust_rename.rs` changed between the two runs and is
excluded from both source and destination comparisons; 442 source files remain.
The three retired shim files and unused files can legitimately have no module place.

Normalization compares call destinations, named type destinations, written import
routes, and module declaration routes. It excludes provenance, hop counts, duplicate
rows and null type destinations. A module row uses its declared name as its local
name, accommodating the old wire format. POSIX `sort -u` and `comm -23/-13` computed
the differences. The CLI does not serialize target-specific ModulePlace rows; module
route agreement is the available wire comparison. The provider fixture separately
checks both target places for a file included by lib and bin.

| Family | Equal | Old only | Provider only |
| --- | ---: | ---: | ---: |
| Calls | 15,281 | 6,939 | 575 |
| Types | 10,395 | 1,695 | 257 |
| Imports | 3,300 | 419 | 3,470 |
| Written module routes | 393 | 142 | 0 |

Every changed route appears in [the TSV](2026-10-05-rust-names-agreement.tsv),
with its source file:line, side, normalized route and contract verdict. Import
locations anchor the matching written `use`, then a glob or declaration when the
wire row has no individual source span. An implicit namespace can anchor line 1.

Of the old-only call rows, 4,277 now abstain `needs_types`, 2,175 were not resolved
by Names (including associated-item paths), 134 have no joined corpus destination,
56 are external, and 297 changed destination or caller attribution. Names does not
lower associated-item receiver types; these calls require Types. The generic
`ambiguous` wire reason also covers unresolved qualified paths in this snapshot.
The 638 old-only String and 474 Vec type routes illustrate corpus name matching
reaching local declarations for std names; the provider does not load std.
The 142 old-only module rows synthesize extern crate namespace routes rather than
written module declarations. Cargo dependencies still resolve through the engine.

The single-run import snapshot exposed missing use offsets in the corpus adapter.
Step 4 corrected it: direct and glob queries use their declaration scope, retaining
RA module ownership and lexical bindings. The TSV marks import verdicts as provider
after scope correction, rather than asserting the earlier root-scope snapshot is
correct. The nested import coordinate fixture passes after correction. The corpus
was not rerun, as requested; these route counts describe the recorded pre-correction
snapshot. The verdicts state the Names contract and do not claim a Types oracle run.
