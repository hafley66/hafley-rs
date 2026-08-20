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

Landed on `refactor/boop-crate-split`. `crates/boop` was 33363 lines in one
crate; it is now five, 34105 lines of `src/` in all (the growth is the crate
headers and the re-export facade).

| crate | owns (real files, after the move) | lines | depends on | the one sentence |
|---|---|---|---|---|
| `boop-store` | `ident.rs`, `rows.rs`, `session.rs`, `query.rs`, `usage.rs`, `activity.rs`, `summary.rs`, `_0_session_graph.rs`, `runtime.rs`, `tail.rs`, `proc.rs`, `bus.rs`, `trail.rs`, `tmux.rs`, `event.rs`, `testing.rs` (`testing` feature), `sql/**`, `tests/wal_three_writers.rs` | 12330 | boop-mux, rusqlite | the database: `~/.agent/boop.db`, its schema, and how transcript bytes become rows |
| `boop-acp` | `channel.rs`, `channel/{acp,jsonrpc,claude,codex,kimi,opencode,tui}.rs` | 2786 | boop-store | how to talk to any agent: an ACP client on a stdio child, or a tmux TUI driver where there is no ACP door |
| `boop-harness` | `harness.rs`, `harness/{claude,codex,kimi,opencode}.rs`, `identity.rs`, `registry.rs`, `worktree.rs`, `tests/fixtures/**`, `tests/bench_grid.rs` | 5232 | boop-store, boop-acp | how to read and re-open what each harness wrote, and the worktree a spawn runs in |
| `boop-proc` | `lane.rs`, `supervise.rs`, `inbox.rs`, `mailwait.rs`, `config.rs`, `host.rs`, `concatmap.rs`, `tests/{parent_death,parent_failure_hail}.rs` | 5421 | boop-store, boop-acp, boop-harness | process control: spawn, supervise, wait, kill, parent policy, the lane mailbox, the embeddable coroutine host |
| `boop` (bin `boop`, the CLI) | `main.rs`, `cli/{mod,job,db,me,mail,debug}.rs`, `debug.rs`, `chat.rs`, `lib.rs` (a facade re-exporting the four above at their old paths), the remaining `tests/*.rs` | 8336 | all four | clap only, plus the one linkable facade a Rust host binds |

**Dependency order is store -> acp -> harness -> proc -> cli**, not the
store -> harness -> acp -> proc this section first guessed. `Harness::open_channel`
returns a `Box<dyn LaneChannel>` and each adapter constructs its own channel, so
the channel is below the adapters, never above them.

Four seams had to move for that order to be acyclic. Each is a move of existing
code, re-exported at its old path:

| seam | from | to | why |
|---|---|---|---|
| `SessionRef`, `KnownSession(s)`, `Ingested`, `ReadChunk`, `Capabilities`, `SendOutcome`, `SpawnSpec`, `OneShotSpec`, `parse_iso_ms` | `harness.rs`, `harness/claude.rs` | `boop_store::session` | `ident.rs` writes rows for a `SessionRef` and cannot depend on the crate above it |
| `sync_session`, `sync_session_with_pid` | `ident.rs` | `harness.rs` | they take a `&dyn Harness`; the cursor half stayed as `ident::sync_session_with`, which takes the projection as a closure |
| `ModelSpec`, `Effort`, `ParentDeathPolicy` | `lane.rs`, `supervise.rs` | `boop_store::session` | `channel/codex.rs` parses a model spelling, and `boop-proc` may not link clap, so the `ValueEnum` derive sits behind the store's optional `clap` feature |
| `opencode::store_path`, `opencode_db_path` | `harness/opencode.rs` | `channel/opencode.rs` | both the adapter and the channel read the opencode store; the channel is the lower of the two |
| `SETUP_SENTENCE`, `start_status_path`, `record_start_status`, `start_preamble`, `brief_with_preamble` | `lane.rs` | `worktree.rs` | `prepare_spawn_dir` records the warm-up status, and every adapter's `spawn` calls it |

Rules for the split, as landed: each crate's `lib.rs` lists its public surface;
`boop-proc` links no clap; `test_support.rs` is `boop-store`'s `testing`
feature; integration tests moved with their crate; `tests/temp_home_rail.rs`
walks every `boop*` crate's `src/` and `tests/` rather than one crate's.

Two SQL-string reaches across a crate seam are marked `// TODO(crate-seam):`
rather than redesigned: `concatmap.rs`'s `context_tokens` (reads `agent_usage`,
`dict_session`) and `cli/db.rs`'s `USAGE_TOTALS_SQL` (reads `agent_usage`,
`model_price`, and `--show-sql` prints it verbatim). Two more are in
`#[cfg(test)]` blocks of `harness/{codex,kimi}.rs`. Everything else reaching
another crate's tables goes through a typed `boop-store` fn.

## 4. Order of work

| # | card | size | blocked_by | why this order |
|---|---|---|---|---|
| 1 | `boop-main-split` (existing, re-scoped): `main.rs` -> `cli/*.rs` by namespace, zero behavior change, byte-identical `--help` per verb pinned | M | - | every later move is a file move; do it while the surface is frozen |
| 2 | `boop-crate-split` (DONE): the five crates above, one commit per crate extraction in dependency order (store, acp, harness, proc, cli), one PR | L | 1 | compile-time boundaries before renaming verbs |
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

## 6. dl6 on top: the rx model for agents (Chris 2026-08-19)

"dl6 is on top of this, we will effectively have an rx model for agents; i also wanted dl6 to have stream controls to be bash/rx like." Recorded as the target; language design stays with Chris (sprefa CLAUDE.md).

### Jobs as subscriptions

| job control | rx | dl6 today | boop row |
|---|---|---|---|
| `job create` | `subscribe()` | base rel arrival (`POST /arrive`) / `sh` demand row | `agent_lane` |
| `jobs` | live subscriptions | `GET /rel/job` | `lane list` |
| `wait <job>` / `$?` | `lastValueFrom`, `finalize` | `finalize/1` (live), `complete/1` (reserved) | result row, typed rc (#32) |
| `wait` (all) | `forkJoin(children$)` | `combine/variadic` (live) | new verb |
| `kill` | `unsubscribe()` | `unsubscribe/1` (reserved) | `lane delete` -> `job kill` |
| `--on-parent-death kill` | `takeUntil(parent$)` | rule over the parent edge + liveness rel | #34 policy |
| mail | `Subject` | base rel fed from outside; `latest/1`, `next/1` | `boop mail` rows, mood render |
| stall / `--timeout` | `timeout(300s)` | clock annotation (`technique(throttle)`), no `timeout` word | supervisor constant |
| retries | `retry(n)` | none | `retrying` / `retry_budget_exhausted` mail (#34) |
| concatmap coroutine | `bufferWhile + pairwise + concatMap` | `resident-coroutine.dl6` (#369) | `host chat` (#27) |

### Stream controls the language has vs lacks (registry `surface/5` rows, `registry.pl:33-193`)

| rx / bash control | status | where |
|---|---|---|
| `latest`, `pre` (sample), `next`, `finalize`, `combine` | live | `registry.pl:33,38,39,40,69` |
| `seq`, `group_concat`, `concat_fold`, `pairwise`, `repeat` | live (manifest `compiled`) | `compile/out/manifest.json` |
| `subscribe`, `unsubscribe`, `complete`, `error` (lifecycle) | reserved, refuse(lifecycle) | `registry.pl:43-46` |
| `zip` | reserved | `registry.pl:41` |
| `scan` | removed word | `registry.pl:190` |
| `take`, `skip`, `timeout`, `retry`, `debounce`, `merge`, `switchMap`, `&` / `wait` / `kill` as words | absent | language forks for Chris |

The boop-job-namespace card builds the CLI half; the dl6 half is a sprefa arc (lifecycle words + the missing controls), gated on Chris, and lands after `/jobs /mail /me` exists as rows so each control has a concrete stream to test against.

## 7. After ACP (Chris 2026-08-19): each card and crate defended or changed

Decision: agents are driven over ACP (one typed channel for every harness); mail / hail / hooks / tui scrape go away; transcripts -> rows stays; tmux is view only (`attach` = window running the harness's `--resume <session>`, closed on detach); harness processes still run per live job.

| card | verdict | why |
|---|---|---|
| boop-main-split (M) | keep, first | a 7383-line main.rs cannot lose four channels cleanly; split by namespace first, then delete |
| boop-crate-split (L) | keep, crates renamed: `boop-store`, `boop-harness` (transcript ingest only), `boop-acp` (the one channel, replaces `boop-mail`), `boop-proc` (job rows, supervise, worktree, parent policy, attach), `boop-cli` | mail crate has no reason to exist once delivery is `session/prompt`; the channel becomes the second biggest surface and deserves its own crate and tests |
| boop-job-namespace (M) | keep, verbs change: `boop job create\|list\|get\|wait\|kill\|rm\|send <job> <text>\|attach`; `boop mail *` deleted; `tell-parent` = `job send --parent`; `me` keeps `whoami\|mood\|favorite` | `send` is `session/prompt`; no inbox, no drain, no hook |
| boop-mail-dir-global-flag (S) | delete the card | the flag's 34 sites go with mail |
| boop-hidden-verbs-retire (S) | keep | still 16 hidden verbs after the fold |
| boop-opencode-acp-channel (high, PR #38 open) | keep as the seed of `boop-acp`, scope widens from opencode to all harnesses after #38 proves one | measured 0/32 opencode turns; ACP is the only typed door opencode has |
| boop-session-mood (done) | keep, render at `job send` time | still the one place agents' format is set |
| boop-parent-failure-hail / parent-death (done) | keep, mail kind -> `session/prompt` to the parent's session, policy unchanged | |
| boop-tell-parent / tell-children (done) | fold into `job send --parent / --children` | |
| boop-start-warm-detect (done) | keep; the preamble becomes the first `session/prompt` | |
| boop-registry-into-sqlite, kind-enums, spawn-one-shape, result-rc-typed | keep, all land inside `boop-proc` / `boop-store` | |
| instant-boop-migration, boop-agent-network-view | keep; they read `boop-store` only | |

Crate table, revised:

| crate | owns | the one sentence |
|---|---|---|
| boop-store | schema, migrations, sync cursors, transcript ingest, typed queries, `sql/**` | the database |
| boop-harness | per-harness transcript formats, session roots, identity ladder, `--resume` command per harness | how to read and re-open what each harness wrote |
| boop-acp | ACP client over stdio child, session map job -> session id, prompt / cancel / permission policy, event -> trace rows | how to talk to any agent |
| boop-proc | job rows, spawn (worktree + boop-start), supervise (stall, parent policy, typed rc), attach window, trail | process control |
| boop-cli | clap per namespace | the binary |
