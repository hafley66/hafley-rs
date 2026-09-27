---
created: 2026-09-26
updated: 2026-09-27
type: bug
status: open
priority: normal
---

# Turn attribution: stale user label at top, assistant span runs over composer and status bar

## Description

## Report (screenshot attached)
Tab "ryi fast and slow" is tmux session `hafley-rs-4`, pane `%375`, 51x179. It runs `boop tui claude` as route `claude-375`, harness claude, no parent (from `boop whoami`). The turn attribution rail is wrong in two places:

| rail label | where it sits | should be |
| --- | --- | --- |
| `t890 user A` | first visible row, which is the tail of an earlier code block (`Box<dyn Iterator<Item = OpResult<Value>> + Send>`) | no label, or the assistant turn that owns that code |
| `t914 assistant E` | the extended span runs past the last assistant row, through the composer (`❯`), the Claude status lines and the tmux status bar, down to the bottom row | ends at the assistant's last row; composer, status lines and tmux status bar carry no turn |

## Second defect: the bound-session tooltip carries no identity
The pane tooltip reads only `claude · bound session` (`instant/src/terminal.ts:274-278`). It omits the boop route, the pane, and the harness's own session id. It should repeat the binding: route `claude-375`, pane `%375`, harness session (the Claude conversation uuid), and the rung that bound it (`env BOOP_SESSION`).

## Where the code lives
- Span extension and chrome exclusion: `@hafley66/boop-xterm` `6_turnVisibility.ts` (`dropTmuxStatusRow`, composer trim) and `2_turnLocate.ts`. Native locate: `boop-turnvis` `locate_visible_turns` (`crates/boop-turnvis/src/lib.rs:407`).
- Tooltip: instant `terminal.ts:274`. Data comes from `PaneSessionBinding` (`boop_mux_session`).

## Reproduction and implementation receipt

Live `%375` was `hafley-rs-4`, 179x51. The read-only `~/.agent/boop.db` lookup found session `99f3ac04-07ff-46cf-97ee-da5965fdae41`, turn 890 (`user`, whose text contains the `Box<dyn Iterator...>` excerpt) and turn 914 (`assistant`). The new `claude_375_does_not_claim_prior_code_or_terminal_chrome` regression failed before the fix with visible turns `[890, 914]` instead of `[912, 914]`. It now retains the prompt marker for 912, rejects the one-row stale user match, and ends turn 914 before composer/status chrome.

`CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/cargo-target cargo nextest run -p boop-turnvis -j 2 -E 'test(claude_375_does_not_claim_prior_code_or_terminal_chrome) | test(golden_fixtures) | test(wrapped_reply_keeps_its_rows)'` passed 3 tests. Formatted with `cargo fmt -p boop-turnvis`.

## Acceptance Criteria
- [ ] Full fixture from this screen (capture of `%375` plus turns 890-914 from the boop store) reproduces both wrong labels in a boop-turnvis test. Current regression is a minimal cut with turns 890, 912 and 914.
- [x] An extended span stops before composer, Claude status lines and tmux status rows.
- [x] A turn label is never placed on a row whose text belongs to an earlier turn's code block.
- [x] Generic `boop-turnvis` matching has no Claude-name branch; the adapter supplies its source-match policy.
- [x] The tooltip shows route, pane, harness session id and rung.

2026-09-27: `t1_harness_boundaries::behavioral_harness_dispatch_stays_in_adapters` reproduced the generic `harness != "claude"` branch in `boop-turnvis/src/lib.rs:499`. Turn matching now takes an evidence policy; the Claude adapter supplies the two-row-or-prompt-marker rule. `cargo nextest run -p boop-turnvis -j 2` passed 10 tests, and the boundary test passed in `cargo nextest run -p boop -p boop-turnvis -j 2`. That full run encountered a separate Codex live-test update prompt at `0.156.1 → 0.157.1`.

2026-09-27 current-tree repro: `claude_375_does_not_claim_prior_code_or_terminal_chrome` passes (1/1), so the turn-label regression is already fixed here. Tooltip implementation and regression tests are committed in Instant worktree branch `fix/turn-attrib-tooltip`, commit `355dfb99` (`Show full pane session identity in terminal tooltip`). The focused Vitest passes 2/2, the two `boop_mux_session` Rust tests pass, and `pnpm exec tsc --noEmit --pretty false` passes. Remaining gate: the full `%375` capture and turns 890-914 fixture is still unchecked.
