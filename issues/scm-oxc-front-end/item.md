---
created: 2026-09-23
updated: 2026-09-23
type: task
status: open
priority: high
epic: scm-language-frontends
labels: [extract]
blocked_by: ['@scm-rust-front-end']
lane: scm-frontends
lane_seq: 1
---

# SCM++ TS and JS OXC per-file cutover

## Description
Move TS, TSX, JS, and JSX single-file OXC parser passes and language-specific syntax projections behind an SCM++ parser feature. Reuse the Rust slice language-row contract and implement its module-syntax capability. Ryi keeps project resolution, TSI inference, and move planning.

## Acceptance Criteria
- [ ] SCM++ feature owns OXC parse and per-file language projection for TS, TSX, JS, and JSX
- [ ] One owned per-file result supplies requested family, fast-SCIP, and module-reference rows without a redundant ryi parse
- [ ] Module references and replacement spellings preserve TS/JS import styles and path rules
- [ ] Existing TS/JS family goldens, module resolution fixtures, and move dry-runs match the pre-cutover baseline
- [ ] Combined-family parse/query counts are recorded and no per-reference parse is introduced

## Tests Run

## Implementation Notes
