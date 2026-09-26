---
created: 2026-09-26
updated: 2026-09-26
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

## Acceptance Criteria
- [ ] Fixture from this screen (capture of `%375` plus the turns 890-914 from the boop store) reproduces both wrong labels in a boop-turnvis test.
- [ ] An extended span stops before composer, Claude status lines and tmux status rows.
- [ ] A turn label is never placed on a row whose text belongs to an earlier turn's code block.
- [ ] The tooltip shows route, pane, harness session id and rung.
