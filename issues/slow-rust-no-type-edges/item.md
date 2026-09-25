---
created: 2026-09-25
updated: 2026-09-25
type: bug
status: fixed
priority: normal
related: ['@ryi-cli-cleanup']
labels: [extract]
closed: 2026-09-25
---

# ryi slow: rust-analyzer SCIP has no relationships, so Rust resolved_type_edge is empty

## Description

## Description
`ryi slow` projects SCIP `is_implementation` relationships onto `resolved_type_edge`. rust-analyzer's SCIP output carries no relationships at all: `ryi scip --raw --records scip_relationship --scip-index <soopy index> --root crates/soopy` prints 0 rows, so `ryi slow` over any Rust corpus writes 0 `resolved_type_edge` rows (soopy: fast 886, slow 0). The TS fixture `tests/fixtures/scip_relationship` yields 4 rows, so the projection itself works.

A Rust oracle for type edges needs another source: `impl Trait for Type` headers read as occurrences of the trait and the self type inside the impl item span, or the rust-analyzer checker tier.

## Acceptance Criteria
- [x] `ryi slow` over soopy writes `resolved_type_edge` rows for its trait impls, origin scip or checker
- [x] the fast/slow type-edge diff is graded like tests/170 grades call sites

## Tests Run

## Implementation Notes

## Comments

### 2026-09-25T15:31:59Z · @claude-m3-slow-oracle

Fixed on ryi/slow-oracle: slow projects the parse's type-edge candidates through SCIP type references (owner name span to the next owner; impl candidates on the impl header line). soopy: 883 type edges agree, fast-only 3, slow-only 2 (impl 2/2). Pinned in tests/RATCHET_TYPES.tsv. rust-analyzer still emits 0 relationships; the TS is_implementation projection is unchanged.
