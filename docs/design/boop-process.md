# boop-process: job control semantics, and the crate split that gets there

Chris, 2026-08-19: "i just want process control semantics from shell/bash, are we close to that after all this? i imagine some re-arranging" and "we need to split boop into better crate responsibilities". Epic: `issues/boop-process`.

## TOC

1. [Bash job control vs boop today](#1-bash-job-control-vs-boop-today)
2. [Target verb surface](#2-target-verb-surface)
3. [Crate split](#3-crate-split)
4. [Order of work](#4-order-of-work)

## 1. Bash job control vs boop today

Measured against `boop 0.0.2 (7f9245d)`, `boop --help` and `boop beep lane --help`.

| bash | boop today | gap |
|---|---|---|
| `cmd &` -> `%1` | `boop beep lane create` -> lane name | none; the lane name is the job id |
| `jobs` | `boop beep lane list` | none |
| `wait %1`, `$?` | `boop beep lane wait <lane>` exits with the lane's rc (3 = died without a row, 124 = timeout) | none |
| `wait` (all children) | nothing | new: `wait` with no args |
| `kill %1` | `boop beep lane delete <lane>` (stop AND forget; works on carcasses since #35) | `kill` and `rm` are one verb; no kill that keeps the row |
| `kill -- -$pgid` | `boop tell-children --body` (mail, #33) | no signal fan-out; `--on-parent-death kill` (#34) is the only cascading kill |
| `fg` / `bg` | `boop beep lane pane` (screen dump), `tmux attach` by hand | no `attach` |
| `trap`, SIGHUP, `nohup`, `setsid` | `--on-parent-death kill\|reparent\|orphan` (#34), typed parent failure mail | none; `orphan` is `nohup` |
| `timeout N cmd` | supervisor stall window 300s, `SPAWN_CHILD_TIMEOUT` 120s, both constants | no per-job `--timeout` at create |
| `a \| b`, stdin | hail / tell-parent / inbox / `wait --me` (mail rows), `host chat` (stdin/stdout JSON) | mail is the pipe; four spellings for one thing |
| `$$`, `$PPID` | `boop whoami`, the parent edge | none |
| `exec` | `boop beep lane run` (what the pane runs) | internal, stays |
| env for the child | `--brief --goal --mood --preset --on-parent-death` | none |

## 2. Target verb surface

Three namespaces. Everything else is deleted or folded. Additive migration: old spellings stay as hidden aliases for one release, then the `boop-hidden-verbs-retire` sweep removes them.

| namespace | verbs | today's spelling |
|---|---|---|
| `boop job` | `create`, `list`, `get`, `wait [<job>...]` (none = all my children), `kill <job> [--signal]`, `signal <sig> [--children]`, `rm <job>` (forget, carcass-safe), `attach <job>`, `pane <job>`, `run` (hidden, pane-only) | `beep lane *`, `beep agent register` (a job with no pane) |
| `boop mail` | `send --to <job> \| --parent \| --children --body`, `recv [--me]` (the inbox drain), `wait <id> \| --me` | `beep hail`, `tell-parent`, `tell-children`, `inbox`, `wait` |
| `boop me` | `whoami`, `mood`, `favorite`, `register` (pane adoption) | `whoami`, `me *`, `adopt` |
| stays | `db`, `debug`, `config`, `host chat` | unchanged |
| goes | `agent`, `concatmap` (coroutine is dl6's, hafley-rs `boop-concatmap-state-in-store`), `beep` as a word, 16 hidden pre-split verbs, 34 `--mail-dir` declarations (one global flag) | |

This is the surface a dl6-generated OpenAPI describes as `/jobs`, `/mail`, `/me`; the fold happens first so the generated spec is clean on day one (sprefa `openapi-clap-uds-lab`, `boop-hosted-in-dl6`).

## 3. Crate split

`crates/boop` today: 33363 lines, `main.rs` 7383 (930 inline test lines, 120 free functions), `ident.rs` 4089. Sibling crates: `boop-mux` (tmux, 1173 lines), `soopy`.

| crate | owns (today's files) | depends on | the one sentence |
|---|---|---|---|
| `boop-store` | `ident.rs` (schema, migrations, sync cursors, `sync_session`), `rows.rs`, `query.rs`, `usage.rs`, `activity.rs`, `_0_session_graph.rs`, `sql/**` | rusqlite | the database: `~/.agent/boop.db`, its schema, and how transcript bytes become rows |
| `boop-harness` | `harness.rs`, `harness/{claude,codex,opencode,kimi}.rs`, `channel.rs`, `channel/**`, `identity.rs` | boop-store (row types only) | each harness's transcript format, session roots, identity ladder, and the rpc channel that drives one |
| `boop-proc` | `lane.rs`, `worktree.rs`, `supervise.rs`, `trail.rs`, `proc.rs`, `runtime.rs`, `host.rs` | boop-store, boop-harness, boop-mux | process control: spawn, supervise, wait, kill, parent policy, boop-start, the per-lane trail |
| `boop-mail` | `bus.rs`, `inbox.rs`, `mailwait.rs`, `event.rs`, routes | boop-store | the pipe: rows addressed to jobs, delivered by pane injection or hook drain, rendered through mood |
| `boop-cli` (bin `boop`) | `main.rs` split per namespace: `cli/job.rs`, `cli/mail.rs`, `cli/me.rs`, `cli/db.rs`, `cli/debug.rs`, `config.rs`, `debug.rs`, `summary.rs`, `chat.rs`, `tail.rs` | all of the above | clap only; no logic that a library caller could want |
| deleted | `concatmap.rs` (1457 lines) once the dl6 coroutine runs (sprefa Phase 3) | | |

Rules for the split: each crate's `lib.rs` lists its public surface; no crate reaches into another's tables by SQL string (store exposes typed fns); `boop-proc` never imports clap; integration tests move with their crate; `test_support.rs` becomes `boop-store`'s `testing` feature. `cargo-semver-checks` stays on CI.

## 4. Order of work

| # | card | size | blocked_by | why this order |
|---|---|---|---|---|
| 1 | `boop-main-split` (existing, re-scoped): `main.rs` -> `cli/*.rs` by namespace, zero behavior change, byte-identical `--help` per verb pinned | M | - | every later move is a file move; do it while the surface is frozen |
| 2 | `boop-crate-split` (new): the five crates above, workspace, one PR per crate extraction in dependency order (store, harness, mail, proc, cli) | L | 1 | compile-time boundaries before renaming verbs |
| 3 | `boop-job-namespace` (new): `boop job *` + `boop mail *` + `boop me *`, old spellings hidden aliases, `wait` for all, `kill` vs `rm`, `signal --children`, `attach`, per-job `--timeout` | M | 2 | the verb table in section 2 |
| 4 | `boop-mail-dir-global-flag` (existing) + `boop-hidden-verbs-retire` (existing) | S | 3 | delete the aliases and the 34 flags after one release |
| 5 | sprefa `boop-hosted-in-dl6`: the OpenAPI for `/jobs /mail /me` generated from dl6 | - | 3 | the generated surface replaces the hand one |

## 5. Why this shape, and the neighbours

### Reasoning

| decision | why |
|---|---|
| bash job control as the verb model | every agent and every human already carries `&`, `jobs`, `wait`, `kill`, `$?`, pipes, `trap`; a model never has to learn boop's nouns, it maps them. A closed verb set is also what a generated surface needs: `/jobs`, `/mail`, `/me` is the whole API |
| mail is the pipe, not stdin | agents run in panes or hooks, not in a pipeline; a row addressed to a job, delivered by pane injection or hook drain, is the only delivery that reaches a model mid-turn. Four spellings today (hail, tell-parent, tell-children, inbox) say one thing |
| `kill` and `rm` split | today `lane delete` both stops and forgets; job control keeps the exit row after the kill, and `jobs` shows it until `rm` |
| `wait` with no args | a coordinator's last line is "wait for all my children"; today it waits one lane at a time or arms `lane wait &` per lane |
| crate split before verb rename | `crates/boop` is 33363 lines, `main.rs` 7383; three of today's four lanes collided on `main.rs`; compile-time boundaries make the next lanes disjoint by construction, and the libs (`boop-store`, `boop-mail`, `boop-proc`) are what a dl6 program or another tool links without clap |
| no server, no daemon (Chris 2026-08-18) | freshness is sync-on-read at rust speed (0.2s); the reactive layer is dl6's, so boop stays a CLI over a SQLite file |

### Neighbours, measured from their own docs (2026-08-19)

| tool | lang | shape | verbs that overlap | what it has that boop lacks | what boop has that it lacks |
|---|---|---|---|---|---|
| herdr | Rust, single binary, headless server + TUI client | tmux-for-agents: workspaces, tabs, panes, status detection (blocked / working / done / idle) by screen matching or agent extension; `HERDR_ENV` socket injected into every session | `herdr worktree create`, `herdr agent send`, `herdr agent wait`, `herdr pane split`, `herdr agent explain` | attach/detach TUI, persistent panes across terminal death, screen-state detection, `explain` for how a state was inferred | transcript rows in SQLite + SQL surface, byte-cursor sync, parent edges + death policy, typed rc + failure mail, mood, `--reclaim` carcass handling, no server by rule |
| cmux | Swift on Ghostty, macOS app | terminal with vertical tabs, per-workspace git/PR/ports, notifications, embedded browser; CLI + Unix socket for create/split/send input/read screen/screenshot | create workspace, send input, read screen (= `pane`) | native UI, browser, notifications ring | everything above; cmux stores no transcripts and has no job/wait/rc model |
| hcom | Rust, single binary, hooks -> SQLite -> hooks | agents message, watch, spawn, fork, resume, kill each other; mid-turn injection between tool calls; MQTT relay cross-machine | `hcom send -b @name`, `hcom kill <name\|tag\|all>`, `hcom list`, `hcom events --wait`, `hcom f` (fork), `hcom r` (resume), `hcom term` | fork/resume of a session, `events --wait` with filters, cross-device relay, tags as broadcast groups | relational transcript store with SQL, parent policy, typed rc/exit codes, worktree lifecycle, mood; hcom's inbox is the closest thing to `boop mail` |
| guild | Go, single binary, SQLite + MCP | shared context/memory + task claims with atomic locks, BM25 + vector search | task claim ~ `job create`, nothing for wait/kill | semantic search over shared memory, MCP server | process control, transcripts, mail, worktrees |
| agent-console | Rust TUI | finds Codex/Claude sessions from the providers' own transcripts and resumes their native UI | reads the same files boop syncs | resume into native UI | the store, sync cursor, everything after read |
| claude-squad, dmux, amux | Go / TS / Rust | one worktree per agent over tmux, detached sessions | `create`, `list`, attach | attach | store, mail, parent model, rc |

Sources: awesome-agent-orchestrators (andyrewlee), herdr posts (coles.codes, dotzlaw.com, mer.vin), cmux (manaflow-ai/cmux README), hcom (aannoo/hcom README), guild (mathomhaus/guild README).

### Where that leaves boop

Closest relatives are hcom (hooks -> SQLite -> hooks, messaging, mid-turn injection) and herdr (panes, wait, worktrees). boop's distinct bets: the store is relational and SQL-queryable (not an event log), the sync is a byte cursor over the harnesses' own transcripts (no hook needed to record), jobs carry parent edges with a death policy and a typed exit row, and the reactive layer is dl6 over that store. What boop should take from them, in this order: `wait` with filters (hcom `events --wait`), `attach` (herdr), `fork`/`resume` of a job's session (hcom), `explain` for how a liveness state was inferred (herdr).
