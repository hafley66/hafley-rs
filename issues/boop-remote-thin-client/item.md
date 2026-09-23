---
created: 2026-09-23
updated: 2026-09-23
type: feature
status: open
priority: normal
related: ['@boop-message-store-spike', '@lane-close-spam']
labels: [domain-boop, intent-design]
---

# Boop on a server: lanes run remote, the Mac is a thin tmux client

## Description

Run every lane and coordinator harness on a server (the DGX Sparks, spark-79f0 / spark-3ee9),
and leave the Mac with the thinnest possible client: a terminal attached to tmux on the server,
plus a CLI that forwards boop verbs. The Mac (M3, 16 GB) currently hosts every harness.

## Measured on the Mac (2026-09-23)

| process | RSS |
|---|---|
| one lane tree (perf-boop-graph-sqlite-only-luna6, 14 children) | 880 MB |
| heaviest lane tree (pid 66373, 17 children) | 1.6 GB |
| `boop` supervisor inside a lane | 228 MB |
| `codex` binaries inside a lane | 204 + 177 + 172 MB |
| `npm`/`npx` wrapper that launches codex | 85 MB |
| all tmux processes on the Mac | 26 MB total (3 processes) |

The tmux client is already small. The RAM is the harness, the boop supervisor, and the npx
launcher. Moving those to the server is the win; the Mac-side client only has to stay small.

## Target shape

- Server: tmux server, boop store (SQLite), lane supervisors, harness processes, worktrees
  (synced source per the sparkup sync card), cargo targets.
- Mac: `ssh`/`mosh` + `tmux attach` (or the `instant` terminal) and a boop CLI that forwards
  verbs to the server (ssh exec, or a local socket per the MessageStore spike).
- Coordinator spawns (`boop tui <harness>`, `lane create`) default to the server; the Mac pane is
  an attach, never a local harness.

## Requirements

1. `boop` gains a host notion: `--host <name>` or config `default-host`; routes record host.
2. Lane spawn, `beep`, `wait`, `ps`, `debug` work unchanged from the Mac against the server.
3. Pane paste / interrupt keys reach remote panes (tmux send-keys on the server).
4. instant attaches remote tmux sessions (tmux over ssh) instead of local ones.
5. Harness launch without npx: run the codex/claude binary directly (drop the 85 MB launcher).
6. Supervisor RSS budget: measure why `boop` holds 228 MB per lane; target < 20 MB per lane
   (one supervisor for all lanes, or a tiny per-lane process).

## Build vs buy (fill before any code)

| candidate | role | Mac RSS | notes |
|---|---|---|---|
| `ssh -t host tmux attach` | thin terminal | measure | zero new code |
| `mosh` | thin terminal, roaming | measure | UDP, survives sleep |
| `et` (Eternal Terminal) | thin terminal | measure | reconnecting ssh |
| `tmux -CC` / iTerm2 integration | native tabs over ssh | measure | iTerm already running |
| `ssh host boop ...` | verb forwarding | ~ssh | zero new code |
| tiny-serve (Rust, ~/projects/tiny-serve) | socket verb server | measure | smol + rusqlite |
| Go single binary | verb forwarder / supervisor | measure | compare RSS and binary size vs Rust release |
| bash + ssh + sqlite3 CLI | verb forwarder | measure | no binary at all; forwarding may not need more |
| C (static) | supervisor | measure | smallest RSS floor; cost is safety and effort |

The wire and message shapes come from one TypeSpec contract (see the typespec skill) so a Go,
Rust, bash or C client reads and writes the same frontmatter / SQLite rows / socket messages
without hand-kept copies.

## Regression tests

- CLI test: `boop lane create --host <test ssh target>` spawns on the remote tmux server and
  nothing harness-like starts locally (process-table assertion on the client host).
- RSS budget test for the supervisor (fails above the budget).

## Related

- @boop-message-store-spike
- @lane-close-spam
