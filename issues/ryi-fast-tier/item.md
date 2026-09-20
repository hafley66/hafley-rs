---
created: 2026-09-20
updated: 2026-09-20
type: epic
owner: hafley66
status: open
priority: normal
labels: [extract]
---

# ryi fast tier: syntax-plane inference legs

## Description

Parent for every fast-mode (tree-sitter, no compiler) inference leg. Every child is gated on scip-ingestion-conformance: no leg lands until ryi can oracle itself against 100 percent SCIP ingestion. Children: local-binding-inference, fast-path-recursive-inference, ts-field-rename.
