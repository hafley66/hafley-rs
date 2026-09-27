---
created: 2026-09-19
updated: 2026-09-27
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
- [ ] `instrument_all` emits a named error for inner macro attributes (stable Rust rejects the syntax before proc-macro expansion; exact repro below)
- [x] time, log, and memory budgets each fail a test that exceeds them
- [x] ring drain emits `drained_at` and `last_event_at` on SIGTERM
- [x] replay re-exec carries the seed and switches the subscriber
- [x] lab minting is an `oh lab new` CLI subcommand that writes a seeded crate and prints its build output
- [x] `default = []`

## Progress receipt

2026-09-27: `oh::test` now holds a 256-event ring, drains event rows with `drained_at` and `last_event_at` on SIGTERM or failure, and re-execs the selected test with the same `OH_SEED` under the direct fmt subscriber. `cargo nextest run -p hafley-observe -j 2 --test bounded_loops --test oh_testkit --test oh_cli` passed 13 tests, including subprocess SIGTERM/replay and CLI surface checks. `oh lab new --title gate-smoke --manifest crates/hafley-observe/Cargo.toml --root /tmp/oh-lab-gate-smoke` created an indexed `lab-20260927-gate-smoke` crate and its offline `cargo check` passed. `cargo nextest run -p sqlite-ext -j 2 --test 2_load_plugins` passed 3 tests with the updated fixture lockfile. Workspace gate `cargo nextest run --workspace -j 2 -E 'not (test(/e2e|live|tmux|tui_sigint|omp_live/))'` passed 1,340 tests, 0 failed, 199 skipped. Stable Rust 1.98.0 reproduces the remaining gate below: compiler error `E0658: inner macro attributes are unstable` occurs before `instrument_all` runs; this acceptance item remains open.

Remaining gate repro:

```sh
repo_root=$(git rev-parse --show-toplevel)
probe=$(mktemp -d)
mkdir -p "$probe/src"
cat > "$probe/Cargo.toml" <<EOF
[package]
name = "oh-inner-attribute-repro"
version = "0.1.0"
edition = "2021"

[dependencies]
oh = { package = "hafley-observe", path = "$repo_root/crates/hafley-observe", default-features = false, features = ["oh"] }
EOF
printf '%s\n' '#![oh::instrument_all]' 'pub fn target() {}' > "$probe/src/lib.rs"
cargo check --manifest-path "$probe/Cargo.toml" -j 2 --offline
```
