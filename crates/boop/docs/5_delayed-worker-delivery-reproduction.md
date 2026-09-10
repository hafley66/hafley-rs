# Delayed worker delivery: E2E reproduction

Bounded reproduction for the 2026-09-10 observation that `feature-f41-ground-chart`'s
final hail (`m-f7538704`, created `14:00:16.030404Z`) and its completion result
(`m-cdb4d52d`, created `14:00:18.692426Z`) were readable in `boop db mail` before
the coordinator's final response but surfaced in the transcript afterward.

Test file: `crates/boop/tests/5_delayed_worker_delivery.rs`.

## Executable and base under test

| item | value |
| --- | --- |
| installed `boop --version` | `boop 0.0.10 (b573718-dirty)` |
| installed executable sha256 | `174c4917b784b3e73ecc6ca4fc749298228f0d8051ef033e58a40174ba324927` |
| worktree HEAD | `1c7752705456b80d4a08d8a20855c0e15e77ddb4` (`fix/delivery-e2e`) |
| base `crates/boop-harness/src/door/codex.rs` | 1137 lines; `CodexDoor::deliver` = `queue_message` only |
| primary `crates/boop-harness/src/door/codex.rs` | 1311 lines; `deliver_message_with` + `thread/turns/list` + `turn/steer` |

The worktree base does not include the uncommitted active-turn steering change in
the primary checkout. The base and the installed executable therefore differ:
the installed binary steers an active turn; this base always queues. Do not read
this test's result as a statement about the installed executable.

## Test command and result

```sh
CARGO_TARGET_DIR=/tmp/boop-delivery-target \
  cargo test -p boop --test main -j 2 -- \
  t5_delayed_worker_delivery::worker_completion_and_hail_take_the_door_while_progress_stays_in_the_mailbox \
  --exact --nocapture
```

Result: `1 passed; 0 failed`.

The test uses the `deliver_door.rs` harness shape (a `Harness` whose `Door` records
the body it is handed) over a live `kind=coordinator` Codex route. It asserts the
routing contract and the ledger stamp, and prints one structured evidence line per
worker row.

## Evidence timeline

Timestamps: `created_at_logical` is the injected message creation time from the
observation run; `mailbox_stamp_iso` is the transition row's `at_ms` converted to
UTC ISO8601; `monotonic_elapsed_us` is measured around `deliver_hail_budgeted`.

| stage | message | kind | rung | outcome | created_at_logical | mailbox stamp | elapsed µs | model consumed |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| dispatch | m-complete | result | door | accepted-by-harness | 2026-09-10T14:00:18.692426Z | 2026-09-10T14:19:10.796Z | 2052 | unknown |
| dispatch | m-hail | request | door | accepted-by-harness | 2026-09-10T14:00:16.030404Z | 2026-09-10T14:19:10.797Z | 1057 | unknown |
| progress-routing | m-yield | yield | mailbox only | held-in-mailbox | 2026-09-10T13:59:00.000000Z | n/a | n/a | n/a |

Both rows append first (`sequence=1`, `appended`) then land the door
(`sequence=2`, `accepted-by-harness`). The `yield` row never opens the door. The
recorder keeps the last body, which is the hail, sender rendered through the
recipient's mood.

No sleeps are used as a timing oracle; the elapsed values are incidental, not
asserted.

## Observed vs inferred cause

Decisive live evidence retrieved after the first draft (read-only,
`~/.agent/boop.db`): the delayed rows were held by the route's door-push budget,
not by the codex queue. Every one of them appends, immediately lands `cooled-off`,
then later lands `accepted-by-harness` on the `door` rung. The cool-off is
`agent_door_blowout` on route `codex-587`: the historical run used a 60s
window, floor 2 live connects, and a 300s cooldown. Acceptance clusters at the
300s boundaries. The production default floor is now 32; `BOOP_DOOR_FLOOR=2`
continues to reproduce this historical policy explicitly.

`agent_delivery_transition` (UTC ISO8601 from `at_ms`):

| message | created | transition | outcome | detail | at |
| --- | --- | --- | --- | --- | --- |
| m-cdb4d52d | 14:00:18.693 | 2 | cooled-off | cooling off for 297s more | 14:00:18.694 |
| m-cdb4d52d | | 3 | accepted-by-harness | door | 14:05:17.648 |
| m-c89409de | 14:00:22.476 | 2 | cooled-off | cooling off for 293s more | 14:00:22.480 |
| m-c89409de | | 3 | cooled-off | 2 door pushes in 60s against 2 live connects | 14:05:17.665 |
| m-c89409de | | 4 | accepted-by-harness | door | 14:10:22.267 |
| m-d53dcc08 | 14:08:55.804 | 2 | cooled-off | cooling off for 81s more | 14:08:55.808 |
| m-d53dcc08 | | 3 | accepted-by-harness | door | 14:10:22.298 |
| m-85f6f2c1 | 14:09:58.977 | 2 | cooled-off | cooling off for 18s more | 14:09:58.981 |
| m-85f6f2c1 | | 3 | accepted-by-harness | door | 14:15:27.467 |
| m-c938f7e8 | 14:10:46.564 | 2 | cooled-off | cooling off for 275s more | 14:10:46.567 |
| m-c938f7e8 | | 3 | accepted-by-harness | door | 14:15:27.498 |

`agent_door_blowout` on route `codex-587`:

| at | pushes | budget | window_ms | cooldown_ms | why |
| --- | --- | --- | --- | --- | --- |
| 14:00:16.034 | 2 | 2 | 60000 | 300000 | 2 door pushes in 60s against 2 live connects |
| 14:05:17.649 | 2 | 2 | 60000 | 300000 | 2 door pushes in 60s against 2 live connects |
| 14:10:22.328 | 3 | 3 | 60000 | 300000 | 3 door pushes in 60s against 3 live connects |
| 14:15:27.499 | 3 | 3 | 60000 | 300000 | 3 door pushes in 60s against 3 live connects |

Observed cause: `m-c89409de` was created at 14:00:22 and accepted at 14:10:22,
which bounds the delay at ~10 minutes, dominated by two back-to-back 300s
cool-offs. The coordinator could read it in `boop db mail` from 14:01 because the
mailbox row existed; the door acceptance that pushes it into the transcript came
only after the cool-offs. The coordinator's observed "surfaced 14:10-14:11" falls
inside that bound. Exact UI/transcript receipt latency is not stored and stays
unknown; the bound is the tool timestamps above.

## Deterministic budget regression

`boop-proc/src/deliver.rs:1349` adds
`explicit_clock_reproduces_two_push_cooloff_and_subsequent_unique_burst`. It
uses the existing `door_verdict` (`:295`) and `door_gate` (`:343`) helpers with
fixed millisecond timestamps and writes accepted `door` transitions directly
through `Store::append_delivery_transition` (`boop-store/src/ident.rs:2886`).
The fixture has one lane and an explicit floor of two, giving this executable
historical timeline:

| clock (ms) | body | gate result | stored effect |
| ---: | --- | --- | --- |
| 1,000,000 | `clock-one` | `Open` | accepted door transition |
| 1,001,000 | `clock-two` | `Open` | accepted door transition |
| 1,002,000 | `clock-three` | `Blowout` via `door_gate` | one trip, `cooldown_ms=300000` |
| 301,001,999 | `clock-four` | `CoolingOff` | no second trip |
| 301,002,000 | `clock-three` | `Open` | first push after expiry |
| 301,003,000 | `clock-four` | `Open` | second push after expiry |
| 301,004,000 | `clock-five` | `Blowout` via `door_gate` | second trip |

The historical floor-2 test passes with its explicit budget and records two
blowout rows at the two fixed trip times. The production default now admits
normal unique bodies through its 32-push bound; the default-policy test below
records that separate policy while retaining the floor-2 reproduction.

## Reviewed bounded correction

The gate remains implemented in `boop-proc/src/deliver.rs:305-336`.
`door_verdict` counts every recent `door` transition before checking whether
the candidate body already crossed the door (`Store::door_pushes_since` at
`boop-store/src/ident.rs:2933`, then `door_pushed_body_since` at `:2946`). Thus
the third distinct body and a repeated body both enter `DoorVerdict::Blowout`,
with the former using the aggregate push explanation and the latter using the
same-body explanation when the aggregate limit has room.

The reviewed correction sets `DoorBudget::default().floor` to 32 at
`boop-proc/src/deliver.rs:241-249`. `BOOP_DOOR_FLOOR` remains an explicit
override, so floor 2 stays available for reproducing the historical test. The
existing `door_pushed_body_since` guard remains independent of the aggregate
count, and `door_gate` keeps the single blowout write and 300-second retry hold
at `boop-proc/src/deliver.rs:343-380`.

`default_floor_admits_unique_progress_and_bounds_the_33rd_push` at
`boop-proc/src/deliver.rs:1499` executes the default policy with an explicit
clock: eight distinct progress bodies are open, a repeated first body is
rejected with the same-body explanation while only eight pushes are stored, and
the 33rd distinct body records one aggregate blowout at 32 pushes. The CLI help
source states the matching default at `boop/src/cli/mod.rs:145`.

Observed (the green test): a worker `result` and a final `request` to a live
Codex coordinator route are routed to the door and stamped `accepted-by-harness`;
a `yield` row routes to `MailboxOnly`. This matches
`boop-proc/src/deliver.rs:493` (`lane_progress_row()` short-circuits to
`MailboxOnly`) and `:507` (a lane route goes to its supervisor).

Inferred (source read, not executable-observed): the base `CodexDoor::deliver`
(`crates/boop-harness/src/door/codex.rs:181`) has exactly one transport,
`queue_message` (`:626`), which shells to `codex queue --remote`. A `codex queue`
row is read at the coordinator's next turn boundary. The ledger rung for that
path is `door` (`Delivered::Injected` mapped at `boop-proc/src/deliver.rs:570`).
The base cannot distinguish "steered into the active turn" from "queued for the
next turn": both report `Delivered::Injected` and `rung=door`. This is a separate
second-order conflation from the cool-off; it does not explain the 5-10 minute
holds, which the transition records already explain.

## Missing observability

| stage | status |
| --- | --- |
| message creation | injected logical clock (no monotonic source in the store) |
| dispatch attempt | observed (rung + elapsed) |
| selected door | observed (`app-server`, harness `codex`) |
| RPC method | unknown in this harness |
| RPC acknowledgement | observed at the ledger level (`accepted-by-harness`); the actual CLI/RPC receipt is not captured |
| mailbox stamp | observed (`at_ms` to ISO8601) |
| actual model consumption | unknown: no observation point distinguishes transport acceptance from consumption |

A missing observation point is marked `unknown`; nothing is inferred as observed.

## Blocker: why the real Codex door transport is not exercised

The task asked for the real door's selected RPC method and acknowledgement. That
requires the real `CodexDoor`, which calls `Command::new("codex")` directly with
no injection seam. Binding a fake `codex` deterministically failed:

- A `#!` script placed first on `PATH` produces `ENOENT` from `Command::output()`
  under this target's `posix_spawn` process resolution, even though the file exists,
  is `0o700`, and executes directly from the shell.
- A compiled stub (copy of `/usr/bin/true`, then a symlink to it) on `PATH`
  produced the same `ENOENT`.
- When the inherited `PATH` is kept and the fake directory is only prefixed, the
  spawn instead resolves the real `~/.local/bin/codex`, which fails on the long
  `unix://` socket path (`SUN_LEN`), i.e. the prefix is not honored.

Repro of the spawn behavior (base, no fix): the child sets `PATH` to the fake
directory and records `spawn_ok=false`, `spawn_err="Os { code: 2, kind: NotFound }"`
for `Command::new("codex")`, while the fake file is present and executable.

A deterministic test of the real door therefore needs an injection seam in
`CodexDoor::deliver` that accepts the RPC transport as a value. The primary's
uncommitted `deliver_message_with(thread, text, rpc, queue)` provides exactly that
seam and its unit tests already cover steer/queue/receipt paths. Porting that
seam into the base is a material code change, outside this reproduction's
"NO fix yet" bound, so it is not attempted here.

## Scope held

No model run, no live control of unrelated sessions, no sleeps as a timing oracle,
no configuration edits, no database clears. One hail to `codex-587` was sent with
`boop beep --as fix-delivery-e2e`; it cooled off under the door budget and was
retried by the drain.
