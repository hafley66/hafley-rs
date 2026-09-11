# Duplicate lane created from stale liveness

Date: 2026-09-11  
Boop: `0.0.10 (1784a2f-dirty)`  
Coordinator: `codex-587`  
Harness: `opencode`  
Model: `openrouter/deepseek/deepseek-v4.1-flash`

## Failure

Boop accepted `feature-f41-game-combat-direct-port2`, then reported all three
signals below while its first model turn was active:

```text
boop beep ps feature-f41-game-combat-direct-port2
feature-f41-game-combat-direct-port2  0  -  -  -  -

boop beep lane pane feature-f41-game-combat-direct-port2
Error: no such tmux session feature-f41-game-combat-direct-port2

boop debug feature-f41-game-combat-direct-port2
liveness  unknown
last turn none
assistant none
tool none
```

Those results caused a recovery lane named
`feature-f41-game-combat-direct-port3` to be created for the same goal and
owned paths. The delayed lane-2 start message arrived after lane 3 had already
been dispatched. Both workers wrote and committed the same crate.

## Timeline

Times use the Boop log timestamps in America/New_York on 2026-09-11.

| Time | Event |
|---|---|
| 14:28:49 | Lane 1 dispatch started. |
| 14:30:33 | Lane 1 reported `rc=129`, killed by SIGHUP before the expected path existed. |
| 14:30:45 | Lane 2 dispatch started. |
| 14:30:47 | Lane 2 registered conversation `agent-fba94aae5eced620`. |
| 14:31 | `boop beep ps` returned PID 0; `lane pane` returned no tmux session; `debug` returned unknown/no turn. |
| 14:32:24 | Lane 3 recovery dispatch started. |
| 14:32:33 | Lane 2's start message finally reached the coordinator. It had already read sources and was drafting signatures. |
| 14:33:07 | Coordinator sent lane 3 a stop message after learning lane 2 was active. Delivery was held for its next turn boundary. |
| 14:35:18 | Lane 3 completed anyway with commit `a186a9e`. |
| 14:36 | Lane 2 completed with commit `238211c`. |

The accepted implementation was lane 2 commit `238211c`, integrated on main
as `fcf750e`. Lane 3's commit was not integrated.

## Reproduction

1. Create an OpenCode lane with `boop beep lane create` and a typed expected
   path.
2. Immediately query `boop beep ps <lane>`, `boop beep lane pane <lane>`, and
   `boop debug <lane>` before the first transcript synchronization completes.
3. Observe PID 0, missing tmux session, `liveness unknown`, and `last turn none`.
4. Wait for the worker's delayed start hail or result row.
5. Observe that the worker was executing during the interval reported as
   unknown and may have committed successfully.

## Stored evidence

Relevant mail rows in `agent_mail`:

| Message | Lane | Meaning |
|---|---|---|
| `m-2a00ea5d` | port2 | accepted dispatch |
| `m-71836be4` | port2 | delayed start report |
| `m-1d702092` | port3 | duplicate recovery dispatch |
| `m-fe351d89` | port3 | stop held for turn boundary |
| `m-2386fbb3` | port3 | completion result |
| `m-3180de88` | port2 | completed implementation report |
| `m-67b66d29` | port2 | completion result |

## Expected behavior

- `lane create` exposes a `starting` state until the supervisor and harness
  process are observable.
- `beep ps`, `lane pane`, and `debug` distinguish `starting`, `live`, `stale`,
  `retired`, and `dead` without using PID 0 as an ambiguous value.
- `last turn none` is labeled `no completed turn` while a first turn is in
  flight.
- Status includes projection age and the last supervisor or harness activity
  timestamp.
- A recovery command does not treat `starting` or stale projection as a dead
  lane.
- A stop delivered during a running turn interrupts before the worker can
  commit more owned-path changes, or reports that the stop is deferred.

## Regression test

Use a fake or replayed OpenCode harness whose first observable transcript row
is delayed:

1. Spawn a lane and hold transcript projection.
2. Assert status is `starting` with the supervisor or spawn PID, never PID 0.
3. Assert a second `lane create` for the same lane or ownership key refuses
   recovery while the supervisor is alive.
4. Release the first start event and assert the same lane becomes `live`.
5. Deliver stop during the active turn and assert the terminal result records
   whether interruption was immediate or deferred.

