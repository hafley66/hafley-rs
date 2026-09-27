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

## Reproduction receipt

Current release `ryii fast` reproduced the row and serialization path. On the first 3,000 lexically sorted `*.rs` files under `~/.cargo/registry/src`, it emitted 1,593,102 JSONL rows (421,958,775 bytes) in 51.53 s, with 1,689,255,936 bytes maximum resident memory. The current implementation collects `Vec<FlatFact>` in `read/project.rs` and `sorted_lines` serializes each row into a `Vec<String>` then globally sorts it (`read/project.rs:522-530, 1758-1767`); SCM capture rows own `text: String` (`read/lang/7_scm_rows.rs:48-50`).

Release baseline (`RUST_LOG=error ryii fast <corpus> > /dev/null`):

| corpus | rows | output bytes | wall | max RSS |
| --- | ---: | ---: | ---: | ---: |
| `crates/sprefa-extract/src` | 56,912 | 10,195,719 | 0.39 s | 174,571,520 B |
| `~/projects/typespec/packages` | 354,417 | 94,619,922 | 2.14 s | 871,464,960 B |
| first 3,000 sorted `*.rs` paths from `~/.cargo/registry/src` via `ryii fast -` | 1,593,102 | 421,958,775 | 51.53 s | 1,689,255,936 B |

The registry input list for the before/after runs was written to `/tmp/fast-rows-registry-3000.list` with `find ~/.cargo/registry/src -type f -name '*.rs' -print | sort | head -3000`. Release outputs were captured at `/tmp/fast-rows-before-{sprefa,typespec,registry}.jsonl`; elapsed time and RSS were captured with `/usr/bin/time -lp`.
