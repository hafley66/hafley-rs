# Boop feature inventory

Work in progress, based on `66cbe8e`. Worktree: `refactor/boop-lifecycle-consolidation`.
Installed help and isolated reproduction receipts live under
`/private/tmp/boop-lifecycle-consolidation-proof-01a08191`.

| Surface | Observed path | Regression / status |
| --- | --- | --- |
| Register coordinator/native | `main.rs::AgentCmd` → `cli/job.rs::run_agent` → `write_route` | Registration drops session and metadata; reproduction recorded |
| Adopt existing pane | `beep lane patch` → `cli/me.rs::run_adopt_with` | Rejects `%pane` with exit 0; forces lane kind; regression pending |
| Delivery | `beep` → `cli/mail.rs::deliver_hail` → `boop-proc::deliver::land` | Lane routes short circuit to supervisor; incident reproduced |
| Shell wrapper | `shell-init bash` → `boop_wrap` → `boop tui` inside tmux; direct harness outside | Divergent ownership paths; consolidation pending |
| Interactive lifecycle | `cli/control.rs::run_native_tui` → `Door::tui_launch` | Session observed once; later identity transitions pending |

Complete CLI/help/test matrix and canonical paths remain in progress.
Recursive installed help capture includes 101 pages (see `3_help-manifest.json`).

## Registration consolidation

`beep agent register` and compatibility `beep lane patch` now call
`cli/me.rs::register_route`. Explicit `--session-id` and `--tmux` are supported
by registration. Supplied fields merge under the existing store transaction;
omitted fields and route kind survive. A new pane attachment is a coordinator.
Existing lane ownership survives patching. Invalid targets return an error.

Removed `run_adopt`, `run_adopt_with`, duplicate pane discovery/fallback and
unreachable adoption hook-removal branch. Hook management remains `inbox hooks`.
Removed the CLI copy of route serialization. Callers use
`boop-store::bus::route_to_value`; legacy registry field spellings remain a
compatibility obligation. Ordinary `write_route` uses the single-row store
upsert instead of rewriting the complete registry through CAS.

Initial incident regressions: 3 failures before changes; `registry_kinds` after
changes: 8 passed, 0 failed. Additional compatibility tests are being run.
