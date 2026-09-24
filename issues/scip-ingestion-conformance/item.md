---
created: 2026-09-19
updated: 2026-09-23
type: feature
status: deferred
priority: high
epic: extract-parity-move-rename
related: ['@k3-kotlin-scip-ratchet', '@fast-path-recursive-inference']
labels: [extract, intent-correctness]
blocked_by: ['@scm-kotlin-front-end']
---

# prove 100% SCIP ingestion conformance per language

## Description

## Description

User-set 2026-09-19: a test must prove 100% SCIP ingestion for every
SCIP-enabled language. A claim of exactness that cannot be measured is not a
claim.

### Measured 2026-09-19, the crate indexing itself

`ryi --family scip crates/sprefa-extract`, binary built from `9b25f783`,
rust-analyzer 34.6s, exit 0, `index.scip` 12.3 MB, output 12.3 MB of jsonl.

Emitted records:

| record | rows |
| --- | --- |
| `scip_fn_edge` | 24902 |
| `scip_local` | 20459 |
| `scip_ref` | 13166 |
| `scip_def` | 8331 |
| `scip_name` | 7479 |
| `scip_edge` | 1419 |
| `scip_callee_type` | 857 |
| `scip_index` | 1 |

Crates appearing across all 79637 symbol-bearing fields of that output:

```
79637  sprefa-extract
    0  core
    0  std
    0  alloc
```

The same strings counted directly in the binary index the run produced:

```
"cargo core "    36589
"cargo std "     11391
"cargo alloc "   19185
"contains()."      678
```

So the index carries 67165 mentions of the three standard-library crates and
678 `contains()` symbol occurrences, and none of them reach a row. The
question "is this call `str::contains` or `Vec::contains`" is answerable from
`index.scip` and unanswerable from this tool's output.

Attach points seen while measuring, not a diagnosis: `scip_rows.rs:174` reads
`index.external_symbols`, and `scip.rs:888-891` documents that a document the
reader cannot read is treated as external to the corpus, which describes every
standard-library document.

### What conformance has to mean

Two different coverages, both measurable, both required.

1. SCHEMA coverage. The SCIP protobuf declares a fixed set of messages and
   fields: `Index{metadata, documents, external_symbols}`,
   `Document{relative_path, occurrences, symbols, language, text,
   position_encoding}`, `Occurrence{range, symbol, symbol_roles,
   override_documentation, syntax_kind, diagnostics, enclosing_range}`,
   `SymbolInformation{symbol, documentation, relationships, kind, display_name,
   signature_documentation, enclosing_symbol}`, `Relationship{symbol,
   is_reference, is_implementation, is_type_definition, is_definition}`. Every
   field is either ingested into a record or carries a written waiver naming
   why. The test counts, prints the ratio, and fails on an unwaived field.

2. INSTANCE coverage. For one real index, every occurrence symbol either
   appears in an emitted row or is counted as dropped with a reason. The test
   prints `ingested / dropped / total` per reason and fails when a reason is
   `unclassified`.

A waiver is data, not a comment: a list the test reads, each entry naming the
field and the reason, so adding a waiver is a reviewable diff.

### Per language

Conformance is per SCIP-enabled language, not global. The indexer registry maps
project markers to indexers (`Cargo.toml` -> rust-analyzer, `tsconfig.json` or
`package.json` -> scip-typescript, `go.mod` -> scip-go). Each wired language
needs its own fixture index committed and its own coverage assertion, because
indexers populate different optional fields.

Kotlin/JVM: no JDK, coursier, scip-java or kotlinc is installed on the
development machine as of 2026-09-19 (`java -version` fails, `JAVA_HOME`
unset, `/usr/libexec/java_home -V` reports none). Whether a JVM indexer is
wired at all is open; see @k3-kotlin-scip-ratchet.

### Fixture discipline

The fixture index is committed, not regenerated per run, or the test measures
the indexer instead of the ingestion. A regenerated fixture needs its diff read
the same way a golden does.

## Acceptance Criteria

- [ ] a committed test prints SCIP schema field coverage as a ratio and the list of unwaived fields
- [ ] a waiver list exists as data, each entry naming the field and the reason it is not ingested
- [ ] a committed test prints instance coverage over a fixture index: ingested, dropped-with-reason, unclassified
- [ ] the test fails when any symbol is dropped without a reason
- [ ] external-crate symbols (`core`, `std`, `alloc`, third-party) are either ingested or waived with a written reason
- [ ] `str::contains` and `Vec::contains` are distinguishable in the output, or the waiver says why not
- [ ] one committed fixture index and one coverage assertion per SCIP-enabled language
- [ ] the per-language conformance number is printable by one command, no ad-hoc script

## Decisions

### 2026-09-20T17:52:18Z · @chris

2026-09-20: punted. Epic, needs mechanical leaf breakdown before any lane. extract-fast-slow-trait-divide is now its child.

### 2026-09-23T21:14:32Z · @codex

2026-09-23: Resume 100% SCIP field and instance coverage after the SCM++ Rust, TS/JS, and Kotlin per-file front ends land. The ingestion epic remains deferred and is blocked by @scm-kotlin-front-end; compiler/SCIP coverage work stays here rather than entering the SCM++ parser epic.

