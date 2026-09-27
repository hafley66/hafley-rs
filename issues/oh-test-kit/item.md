---
created: 2026-09-19
updated: 2026-09-26
type: feature
status: open
priority: normal
labels: [observability]
---

# oh: the observe test kit and lab minter

## Description

`hafley-observe` becomes the one repo for codebase-maintenance machinery: tracing,
OTLP, log format, test harness, lab minting. Everything conditional behind features.

## Why

Testing is observation. A separate test crate would duplicate the subscriber stack,
the span vocabulary, and the counting layer that already live here.

## Surface

Namespace is `oh`. Four attributes:

| attribute | job |
|---|---|
| `oh::test` | registers a test with time, log, and memory budgets |
| `oh::budget` | per-function deadline, detection not preemption |
| `oh::instrument_all` | spans every fn in an inline `mod` or `impl` block |
| `oh::skip` | opt a fn out of `instrument_all` |

A bare `#[test]` resolves to `oh::test` when the caller writes `use oh::test;`.
Verified: a user proc-macro attribute named `test` shadows the builtin, and the
expansion re-emits `#[::core::prelude::v1::test]` underneath.

## instrument_all, measured limits

Works on `mod foo { ... }` and `impl Foo { ... }`. Both of these are nightly-gated
on stable and must emit a named error instead of failing silently:

- `#[oh::instrument_all] mod foo;` gives `E0658: file modules in proc macro input are unstable`
- `#![oh::instrument_all]` gives `E0658: inner macro attributes are unstable`

This repo's own crates use `#[path = "N_name.rs"] mod x;`, so the reachable surface
is `impl` blocks and inline module wrappers, not one line per file.

## Budgets

| budget | mechanism |
|---|---|
| time | `Instant` at entry; `cargo nextest` SIGTERM as the backstop |
| logs | per-callsite counter, so a loop naming one line is caught, not just total chattiness |
| memory | global allocator wrapper, `LIVE` and `PEAK` atomics, no polling, no thread |

## Drain and replay

Run 1 buffers through a bounded ring, quiet. On failure or SIGTERM the handler
flushes and stamps `drained_at`; the gap between that and `last_event_at` separates
a runaway loop (small gap, full ring) from a block (large gap, last event names the
line). Then the harness re-execs itself with logging direct to console, same seed,
because at that point the ring is suspect too. Replay is default-on and
config-overridable per test.

A timeout replay is not the same run. Logging changes timing and allocation. The
output must say so rather than imply the runs match.

## Lab minting

`labs/new-lab.sh` from `sqlite_ivm` moves here and becomes a CLI subcommand with a
build output. It datestamps and indexes lab directories, enforces the title-card
naming rule, and seeds an ISO lab whose dependency versions are pinned from the
entry point's manifest but which carries no path dependency on it.

## Also

`default = []`. Today `default = ["otlp", "sqlite"]` drags rusqlite and a reqwest
HTTP client into every consumer.

## Acceptance Criteria

- [x] `oh` attribute crate exists with `test`, `budget`, `instrument_all`, `skip`
- [x] `use oh::test;` plus bare `#[test]` works end to end
- [x] `instrument_all` emits a named error for file modules (macro unit test)
- [ ] `instrument_all` emits a named error for inner macro attributes (stable Rust rejects the syntax before proc-macro expansion)
- [x] time, log, and memory budgets each fail a test that exceeds them
- [ ] ring drain emits `drained_at` and `last_event_at` on SIGTERM
- [ ] replay re-exec carries the seed and switches the subscriber
- [ ] lab minting is a CLI subcommand with a build output (`find .. -iname '*new-lab*'` found no source script in this checkout)
- [x] `default = []`

## Progress receipt

2026-09-27: The code remains hosted in `crates/hafley-observe` (`src/11_testkit.rs`, `tests/oh_testkit.rs`) with proc-macro attributes in the sibling `hafley-observe-macros` crate. The macros and testkit are gated by `hafley-observe/oh`, selected only through its dev-dependency. Subscriber features remain opt-in; `boop` selects `fmt` for its log output. `cargo nextest run -p hafley-observe --test oh_testkit --test bounded_loops -j 2` passed 9 tests, including the universal loop scan. Inner-attribute diagnostics, SIGTERM drain/replay, and lab minting remain open as listed above.
