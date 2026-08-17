# Validate the concatmap send-only + turn-event refactor

Date: 2026-08-17. Status: built; crate green, e2e liveness RED (receipts pass).
Follows `2026-08-16-concatmap-e2e-receipts.md`,
`2026-08-17-turn-event-system.md`, `2026-08-17-strip-concatmap-output-path.md`.

## TOC

1. Why
2. Steps
3. Acceptance

## Why

The turn-event refactor (`poll_turn`/`TurnEnd` -> `next_event`/`TurnEvent`)
touched 8 files (`channel.rs`, `channel/{claude,codex,kimi,opencode,tui}.rs`,
`supervise.rs`, `concatmap.rs`). The strip removed the `--out` path. Only the
21 concatmap unit tests and a compile check have run so far. This validates the
whole crate and proves the receipt e2e (the user's ask) actually delivers two
one-way user bundles and a liveness reply through a live model.

## Steps

1. `cargo test -p boop` — full unit + integration suite (supervise, channels,
   lane, bench_grid, lane_wait_exit, registry_kinds).
2. `cargo test -p boop --test concatmap_e2e -- --ignored` — the receipt e2e,
   flash4 -> gem37 -> haiku -> luna fallback chain.

## Acceptance

| check | expected | actual |
| --- | --- | --- |
| full crate suite | 0 failures | green (lib 21 + bench_grid 2 + lane_wait_exit 5 + registry_kinds 3) |
| e2e receipt | exactly 2 user turns == bundles | pass, all 4 models |
| e2e liveness | >=1 assistant reply | RED, all 4 models (`reply=false`) |

## Diagnosis

The receipts (one-way user turns) land in every model attempt; only liveness
fails. The turn-event fold restores serial wait-for-done: `rewrite` blocks on
the TUI channel's `TurnDone` (`channel/tui.rs:319`, pane-idle detection)
before the next bundle. In this headless tmux run that event does not fire, so
the model reply never completes and is not ingested as an assistant turn
inside the 60s `RECEIPT_TIMEOUT`. Plan 1 was green under fire-and-forget, which
submits both bundles without waiting and lets replies land independently.

## Options (open)

| option | trade |
| --- | --- |
| raise `RECEIPT_TIMEOUT` | may still fail if TurnDone never fires headlessly |
| fix TUI pane-idle detection under headless tmux | the real fix, needs opencode store state at failure |
| dump store + `next_event` trace on e2e failure | diagnostic, before any fix |
| treat liveness as env-flaky and assert receipts only | accepts the model reply gap |
