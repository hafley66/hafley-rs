---
created: 2026-08-17
updated: 2026-08-19
type: improvement
status: closed
priority: normal
epic: boop-process
labels: [domain-boop, intent-implementation]
size: M
---

# main.rs is 6061 lines with 930 inline test lines and 120 free functions

## Description

`main.rs` is 6061 lines, 930 of them an inline `mod tests` (`:2772-3701`), leaving ~5130 production lines and 120 free functions. Section-marker comments already mark the seams to cut on.

| field | value |
|---|---|
| audit row | section 9, row 6 |
| cost | M |
| needs Chris | no |

Sites:

- `crates/boop/src/main.rs`

## Fork

Do this AFTER the small fixes land, or it conflicts with every one of them.

## Acceptance Criteria

- [ ] Split follows the module table in audit section 7 (`cli/beep.rs`, `cli/db.rs`, `cli/spawn.rs`, `cli/mail.rs`, `cli/route.rs`, `cli/supervisor.rs`, `cli/read.rs`, `cli/ps.rs`, `cli/config.rs`, `cli/doctrine.rs`).
- [ ] The 930 inline test lines move to `tests/cli_*.rs`.
- [ ] `main.rs` keeps only the clap tree and `main()`.
- [ ] Pure move: no behavior change, `cargo test -p boop -j4` green with the same test count.

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.

## Comments

### 2026-08-19T14:35:33Z · @sprefa-coordinator

Re-scoped under epic boop-process (2026-08-19): main.rs -> cli/{job,mail,me,db,debug}.rs by namespace, zero behavior change, byte-identical --help per verb pinned; first card of docs/design/boop-process.md section 4.

### 2026-08-20 · @sprefa-coordinator

Landed as #40 (4c3c490): main.rs 7383 -> 1786, cli/{mod,job,mail,me,db,debug}.rs, help byte-identical (84 screens), 461 tests unchanged, clippy rc=0. Tests stayed as #[cfg(test)] mods beside their code (bin-crate privates unreachable from tests/). temp_home_rail.rs STORE_WAIVED renamed to the cli paths.
