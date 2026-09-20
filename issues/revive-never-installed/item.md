---
created: 2026-09-18
updated: 2026-09-18
type: bug
status: open
priority: high
---

# revive shipped green and was never installed; two e2e legs red under load

## Description

## Description

`boop beep lane revive` is written, committed and green, and has never been installed. The binary on PATH predates it, so the verb does not exist for the user.

```
$ boop beep lane revive --list
error: unrecognized subcommand 'revive'
```

| fact | value |
|---|---|
| installed binary | `~/.cargo/bin/boop`, built 17:21 |
| the commit | `25ad65f3`, committed 21:57, local only |
| source has the verb | `crates/boop/src/main.rs:1206` `Revive {` |
| commit status | `Boop-Status: done`, 12 files, +1694 |
| local main | ahead 9 of `origin/main` (`39189edb`) |

## Why it stayed uninstalled

`just install-boop` runs `crates/boop/scripts/install-guard.sh`, which refuses unless HEAD is an ancestor of `origin/main` and no tracked file differs from HEAD. The working tree carries 41 modified tracked files from concurrent agents, and the 9 commits were never pushed. Both conditions fail, so the install silently never happened after the lane reported done.

The gap is structural: a lane can report `Boop-Status: done` on work that the install rail will then refuse, and nothing surfaces that.

## Gate re-measurement (clean detached worktree at 25ad65f3)

| leg | result |
|---|---|
| `boop-harness --lib door::` | 57 passed, 0 failed, 2 ignored |
| `boop --bins cli::control` | 16 passed, 0 failed |
| `tui_revive_e2e` | 2 passed, 1 FAILED |
| `lane_retire_revive` | 1 passed, 0 failed |
| `tui_sigint_e2e` | 1 passed, 2 FAILED |

The commit claims all five green on this machine at 21:57. The re-measurement ran at load average 10.07, and both red legs are tmux-kill e2e that wait on route re-registration timeouts, the exact shape that flakes under load. The standing law says never measure a leg from the whole gate under lane load. Re-measurement of the two red legs alone is in flight; that result decides defect versus load.

## Acceptance Criteria

- [ ] The two red legs re-measured alone, three times, on a quiet machine
- [ ] If genuinely red, the failing case named with its panic
- [ ] 9 local commits pushed to `origin/main`
- [ ] `just install-boop` run from a clean tree
- [ ] `boop beep lane revive --list` answers
- [ ] A rail exists so a lane reporting done on install-blocked work surfaces it

## Implementation Notes

Grading worktree at `/Users/chrishafley/projects/hafley-rs-grade-revive`, detached at `25ad65f3`. Remove it when this closes.

## Decisions

### 2026-09-19T02:19:50Z · @claude-opus-5

Both red legs are the claude case only. Codex and opencode pass every run.

Cause candidate: `claude` on PATH is `/Users/chrishafley/.local/bin-ansi/claude`, a 4520-byte bash wrapper (claude-ansi). It resolves the newest installed version, repatches under a lock if the updater moved on, starts the ANSI proxy, waits for it, then execs the real binary at line 148. The real binary at `~/.local/bin/claude` is shadowed. Codex and opencode have no wrapper.

Both failures are `wait_for_screen` deadline timeouts:

- `tui_revive_e2e::a_dead_coordinator_pane_revives_on_its_session_claude` at `crates/boop/tests/tui_revive_e2e.rs:306`
- `tui_sigint_e2e::ctrl_c_leaves_no_orphaned_backend_claude` at `crates/boop/tests/tui_sigint_e2e.rs:217`

That is the shape a slow harness startup produces, not a wrong-answer assert.

`tui_sigint_e2e` went 2 failed then 1 failed across two runs at load average 10.07, so it is load sensitive on top of the wrapper cost.

Next: confirm the wanted string from the assert, then re-measure with PATH pointed at `~/.local/bin/claude` on a quiet machine. If that turns both legs green, the defect is the test's dependence on whatever `claude` resolves to, and the rail is for the e2e to resolve the real binary rather than inherit PATH.

### 2026-09-19T02:22:18Z · @claude-opus-5

Root cause confirmed. Not the wrapper, not load, not the banner string.

The test gives claude a scratch `HOME` (`tui_revive_e2e.rs:86,102` set `BOOP_READER_HOME` under the case root). A claude with a fresh `HOME` never prints its banner. It shows first-run onboarding instead. Measured directly on a private tmux socket with `HOME` pointed at an empty temp dir, claude 2.1.277:

```
 Let's get started.
 Choose the text style that looks best with your terminal
```

So `wait_for_screen(..., "Claude Code v")` waits for a banner sitting behind the onboarding theme picker, and times out every run. Deterministic. Claude-only, because codex and opencode have no equivalent first-run gate, which is exactly the pass/fail split observed: claude fails every run, the other two pass every run, in both legs.

Two earlier hypotheses are dead and should not be re-tried:

- The claude-ansi PATH wrapper is not the cause. It runs in 0.022s, faster than the shadowed real binary at 0.095s.
- The banner string is correct. `Claude Code v` is present 29 times in the 2.1.277 bundle and renders as `Claude Code v2.1.277` under a normal `HOME`.

#### The fix, already written elsewhere in this repo

`crates/boop/tests/5_live_harness.rs:227-237` seeds the scratch config before launching:

```json
{
  "firstStartTime": "2026-01-01T00:00:00.000Z",
  "firstStartVersion": "2",
  "hasCompletedOnboarding": true,
  "lastOnboardingVersion": "999.0.0",
  "projects": { "<workspace>": { "hasTrustDialogAccepted": true } }
}
```

`crates/boop-harness/src/harness/claude.rs` also knows `hasCompletedOnboarding`. Neither `tui_revive_e2e.rs` nor `tui_sigint_e2e.rs` seeds it.

#### Why the lane reported green

The commit claims all five legs green at 21:57 on this machine. The seed is absent from the test file, so either the lane ran against a scratch HOME that inherited state some other way, or the claude build changed under it. claude 2.1.277 is dated Sep 18 14:09, before the lane ran, so a mid-run version bump is not the explanation. Worth one look before trusting any future `Boop-Check` line from that lane.

#### Next

- [ ] Seed the scratch config in `tui_revive_e2e.rs` and `tui_sigint_e2e.rs`, copying the `5_live_harness.rs` block
- [ ] Re-measure both legs three times on a quiet machine
- [ ] Then push the 9 commits and run `just install-boop`
- [ ] Rail: a shared test helper for the seeded scratch HOME, so the next harness e2e cannot forget it

### 2026-09-19T03:46:29Z · @claude-opus-5

Root cause settled, and it is environmental. Boop's revive code was never implicated.

#### What actually happens

`claude` on this machine's PATH is `/Users/chrishafley/.local/bin-ansi/claude`, the claude-ansi wrapper. It resolves the binary from `$HOME/.local/share/claude/versions`. Every harness e2e gives claude a scratch `HOME`, where that directory does not exist, so the wrapper exits immediately:

```
$ HOME=$(mktemp -d) ~/.local/bin-ansi/claude --version
claude-ansi: no runnable claude binary under /var/.../T/tmp.../.local/share/claude/versions
```

The pane closes, `capture-pane` returns an empty string, and `wait_for_screen` spends its full deadline waiting for a banner from a process that already exited. Codex and opencode have no HOME-dependent PATH wrapper, which is the entire claude-only pass/fail split.

The decisive artifact was the assert's screen dump being empty. Every theory tried before reading it was wrong.

#### Proof

| tree | env | result |
|---|---|---|
| pristine 25ad65f3 | PATH claude | FAILED 3/3, 90s timeout each |
| pristine 25ad65f3 | `CLAUDE_BIN=~/.local/bin/claude` | ok, 6.36s |

#### Full gate at 25ad65f3 with CLAUDE_BIN, clean detached worktree

| leg | pass 1 | pass 2 | pass 3 |
|---|---|---|---|
| `tui_revive_e2e` | 3 passed | 3 passed | 3 passed |
| `tui_sigint_e2e` | 3 passed | **2 passed 1 FAILED (80s)** | 3 passed |
| `boop-harness --lib door::` | 57 passed, 2 ignored | | |
| `boop --bins cli::control` | 16 passed | | |
| `lane_retire_revive` | 1 passed | | |

Revive's own leg is 3 for 3 across three passes, all three harnesses. The feature is sound.

#### Dead theories, recorded so nobody retries them

- Load flake: reproduced 3/3 at an identical 90s timeout.
- Stale banner string: `Claude Code v` appears 29 times in the 2.1.277 bundle and renders as `Claude Code v2.1.277`.
- Onboarding intercepting the banner: seeding `hasCompletedOnboarding` and `lastOnboardingVersion` plus writing `$HOME/.claude.json` changed nothing, 3/3 still red. That edit was dropped rather than shipped.
- Wrapper startup cost: 0.022s, faster than the shadowed real binary at 0.095s. This measurement was taken under the real `HOME` and is what caused the wrapper to be wrongly cleared early.

#### Rails worth building

- [ ] `tui_sigint_e2e` is flaky on its own, 1 red in 3 passes at an 80s timeout shape. The revive commit does not touch that file, so this is pre-existing and deserves its own issue.
- [ ] `resolve_executable` should reject a claude that cannot run under the scratch HOME, or `mock_tui_launch` should probe it once and fail fast. A blank pane for 90 seconds teaches nothing.
- [ ] `wait_for_screen` should say the pane is dead or empty rather than printing an empty screen dump after the wanted string.


