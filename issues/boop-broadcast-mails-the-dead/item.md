---
created: 2026-09-26
updated: 2026-09-26
type: bug
status: open
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
- [ ] One golden test: a registry fixture with 1 live pane (real tmux + a stub harness process), 1 dead-pid process route, 1 harness-less route, 1 door route with no socket, and 1 pane id reused by a different process. Snapshot of stdout, stderr and the registry after the send: only the live pane lands; stdout has 2 lines; stderr has no WARN; the dead routes are marked.
- [ ] `boop beep shout` on the 2026-09-26 registry prints ≤ 16 lines.
