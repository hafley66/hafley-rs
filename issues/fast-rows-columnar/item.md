---
created: 2026-09-25
updated: 2026-09-25
type: improvement
status: needs-decision
priority: normal
labels: [extract]
---

# fast rows: arena + columnar spans, serialize straight to the sink

## Description

## Description

Every fast row is an owned `FlatFact` (String paths, names, kinds per row), collected into one `Vec<FlatFact>`, serialized to a `Vec<String>`, sorted, then written. Measured, release, 3000 registry .rs files (1.55M output rows):

| stage | wall |
| --- | --- |
| project_facts (row building, now parallel) | 411ms |
| sorted_lines (serialize + sort, now parallel) | 121ms |
| write | 57ms |
| gaps between them (drops of the Vec<FlatFact> / Vec<String>) | ~100ms |

Top self-time frames of the same run include `Vec<String>` collects (~1000 samples across two frames), `_platform_memmove` 458, SipHash `write` 232, `mi_malloc`/`mi_free` 364, `serde_json::format_escaped_str`. scm Capture rows carry an owned `text: String` per capture (7_scm_rows.rs) though the text is a slice of the file.

Target shape (user direction 2026-09-25): per-file arena (oxc_allocator is linked), rows as flat columns of u32 spans + interned name ids + file id, text taken from the source buffer only at output, line/col from newline offsets at output, serialized straight from the columns to the JSONL or sqlite sink; ordering from sorted inputs instead of a global sort.

## Acceptance Criteria
- [ ] no per-row String allocation on the fast path
- [ ] no global sort of serialized lines; output order still deterministic
- [ ] release numbers before/after on sprefa-extract/src, typespec/packages and the 3000-file registry corpus

## Plan

Capture release baselines for the three named corpora, then move fast-path rows to per-file arena columns keyed by file id and byte spans with interned names, deriving text and line data at output; preserve deterministic ordering from the input sequence and compare allocations, wall time, and output bytes after each migration slice.

## Decisions

Should the first migration slice cover SCM capture rows only, or convert every fast-path fact family in one pass?
