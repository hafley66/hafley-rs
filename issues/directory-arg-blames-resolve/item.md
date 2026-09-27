---
created: 2026-09-18
updated: 2026-09-26
type: bug
status: fixed
priority: normal
epic: extract-parity-move-rename
labels: [extract, artifact-cli, phase-refinement-1, intent-correctness, component-usage]
---

# a directory argument reports a --resolve error when --resolve was never passed

## Description

## Description

Two argv shapes for "run over this tree" both fail, and neither message names a
command that works.

A directory argument names a flag the caller never passed:

```
$ extract --family cst src
extract: src is a directory; --resolve takes files, so expand the tree with a shell glob or find
exit=0
```

Three defects in one line. `--resolve` is not in the argv. The exit code is 0 on
a run that produced no output. The remedy it suggests fails:

```
$ extract --family cst $(find src -name '*.rs')   # 101 files
Error: "exactly one PATH is required unless --resolve is given"
exit=1
```

So the documented escape from the directory error lands on a second error whose
own remedy is the flag the first error blamed. Adding `--resolve` changes the
pipeline, not just the input set, which is the wrong trade for a caller who wants
one family over many files.

Workaround used on 2026-09-18 to collect declarations across the crate: 101
single-file invocations in a shell loop, attributing the path outside the tool.
That is 101 process spawns to answer one question.

## Acceptance Criteria

- [x] A directory argument either walks the tree or exits non-zero. Never exit 0 with no output.
- [x] No error message names a flag absent from argv.
- [x] Multiple PATH arguments work for a per-file kind run without requiring `--resolve`.
- [x] The tested command lines work without adding `--resolve`.
- [x] A test asserts the exit code for the directory case.

## Tests Run

`cargo nextest run --features cli -j 2 --test all -E 'test(/^t_50_cli_crawl_defects::kinds_accept_directory_and_multiple_paths_without_resolve$/)'` passed.

## Implementation Notes

The current `ryi` input expansion already walks directory arguments. The regression test pins both a directory and two explicit files with `--kinds cst`.
