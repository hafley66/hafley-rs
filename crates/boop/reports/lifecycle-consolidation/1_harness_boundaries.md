# Harness boundary evidence

Work in progress. `boop-harness::registry::Registry::discover` registers Claude,
Codex, Kimi and OpenCode. Existing boundaries are `Harness`, `Door`,
`LiveSessions` and `LaneChannel`.

Baseline rg scan: 219 lines, including tests embedded in source. Raw output:
`/private/tmp/boop-lifecycle-consolidation-proof-01a08191/1_harness-baseline-rg.txt`.
This count is a candidate inventory, not a count of behavioral dispatches.
Classification by symbol, role and static schema obligations remains pending.

| Behavior | Current owner / migrated consumer | Evidence |
| --- | --- | --- |
| Native Codex server launch, configuration forwarding, selected-thread response decoding | `boop-harness::door::codex`; `cli/control` consumes typed `NativeTuiEvent` | 11 Codex adapter tests, live resume/settings/clear receipts |
| Route endpoint interpretation | `LiveSessions::live_session_for_route`, overridden by the Codex door; `boop-proc::deliver::live_session` delegates | Private backend route delivers without waiting for legacy state DB discovery |
| Native process resource cleanup | `NativeTuiPlan` owns the backend and observer; CLI handles TERM/HUP | Normal exit and TERM remove owned sockets; remaining frontend/error/SIGKILL gaps in report 3 |
| Transcript decoding, tool names, process discovery, native child formats | Existing four harness adapters | Harness suite: 170 passed; machine-dependent Claude live-list test ignored |

No new behavioral harness enum branch was added to CLI/process code for native
Codex launch or observation. The complete behavioral/static classification and
enforceable guard remain required work.
