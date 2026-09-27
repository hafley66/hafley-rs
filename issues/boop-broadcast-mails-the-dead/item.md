---
created: 2026-09-26
updated: 2026-09-27
type: bug
status: fixed
priority: high
---

# shout / scream / revive mail routes that are not running, and print a line per dead route

Branch: `bug/the-gang-mails-the-dead`

## Description
One `boop beep shout` on 2026-09-26 printed `15 landed, 0 cooled-off, 101 no-route`: one stdout line per no-route, with tracing `WARN` lines interleaved. 101 of the 116 recipients were not running.

| no-route reason seen | example routes | why it was selected |
| --- | --- | --- |
| `no live codex session for …` | `codex-process-89306`, `extract-field-reports-parent`, `tsp-sql-parent-20260909` | registry row still present; the process is long gone |
| `route … names no harness` | `codex-root`, `herd`, `luna-native`, `feat-one-sqlite-mailbox` | `kind` = coordinator/native with no tmux → `Reach::DoorOnly`, counted as connected "while registered" |
| `codex door … connect Codex app-server socket …/app-server-control.sock` | `codex-projects`, `game3-overnight` | door-only route; the socket file does not exist; each send also logs 2 `WARN`s and a 120s cool-off |
| `harness takes no door mail; pane %0` | `omp-0`, `omp-2`, `omp-453` | tmux target alive → `Reach::LivePane`, but that pane id does not prove the harness is still in it |
| `harness takes no door mail` | `omp-process-13068` … (15 rows) | registered process routes, pid not checked |
| `no live claude/opencode session for …` | `rxjs-284a`, `sprefa-298b`, `opencode-0` | same as the first row |

## Where
| step | file | today |
| --- | --- | --- |
| selection | `crates/boop/src/cli/shout.rs:64` `connected` | keeps `Reach::LivePane` and `Reach::DoorOnly` |
| reach | `shout.rs:49` `reach_of` | `LivePane` = `mux.target_alive(target)`; `DoorOnly` = `kind ∈ {coordinator, native}`; no pid, heartbeat or door probe |
| deliver + print | `shout.rs:~244-272` | `println!` per recipient: `landed …`, `cooled-off …`, `no-route …`; then the totals line |
| door cool-off | `crates/boop-proc/src/deliver.rs` | `WARN door unreachable; cooling off the route` + `WARN door call failed` per door route per send |
| registry | `boop::registry::Registry` | rows are never reaped when the pid, pane or door dies |

## Want
- **Selection is liveness, not registration.** A route is a recipient only if a live proof passes at send time:
  - pane routes: the tmux target exists AND the pane's process tree contains the route's harness (`#{pane_pid}` → children match `harness.bin`)
  - process routes: `kill(pid, 0)` succeeds, and the pid's command matches the harness
  - door-only routes: the door transport connects (socket exists / ACP session live), checked once per send without a cool-off entry
  - a route with no harness is never a recipient
- **Reaping.** Any route that fails its proof on 2 consecutive sends (or older than N hours with no proof) is marked dead in the registry and drops out of `connected`. A revive, or `boop tui`, re-registers it.
- **stdout discipline** (shout, scream, revive):
  - stdout: one line per *landed* or *failed-while-live* recipient, then the totals. Never-live routes print nothing by default; `--verbose` lists them.
  - stderr: tracing only. No `WARN` for routes that were filtered out before delivery.
  - `--json` prints one object: `{landed:[…], failed:[{route, why}], skipped:n}`.
- **revive.** Same proof before it spends a spawn: a revive of a route whose harness is already running in its pane is a no-op with one line saying so.

## Acceptance Criteria
- [x] The route proof matrix covers a live pane, dead pid, harness-less route, unreachable door, and a pane occupied by a different session. The real-TUI scream integration checks two stdout lines, no stderr WARN, and delivery only for a proved live route. The dead-route state test checks the two-miss transition and reset on proof or re-registration.
- [x] Never-live routes are omitted by default, verbose output lists skipped routes, and JSON emits one result object with landed, failed, and skipped fields.

## Decisions

### 2026-09-27T02:48:27Z · @codex

Repro receipt (2026-09-27): `cargo nextest run -p boop -j 2 -E 'test(/connected_routes_are_live_panes_and_paneless_coordinators/)'` passes while asserting that `coord-paneless` and `native` are selected as connected with `alive` never called for them. The card already specifies the resolution: after two failed liveness proofs, retain the route marked dead; revive or `boop tui` re-registers it. No user decision is pending.

### 2026-09-27 · @codex

Completion receipt: route liveness proof, persistent two-miss state, recipient output modes, and revive no-op use the shared harness proof. `cargo nextest run --workspace -j 2 --status-level fail -E 'not (test(/e2e|live|tmux|tui_sigint|omp_live/))'` passed 1,355 tests (203 skipped); `cargo nextest run --features cli -j 2 --test all` in `crates/sprefa-extract` passed 1,114 tests (18 skipped). The `boop` workspace suite includes `scream_interrupts_each_real_tui`, the route proof matrix, two-miss state transition, and `tui_revive_e2e`.
