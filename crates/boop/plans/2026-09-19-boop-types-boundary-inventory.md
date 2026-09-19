# boop-types: boundary inventory

Measured 2026-09-19 against `hafley-rs` at the working tree. 147 `.rs` files
across 8 crates: `boop`, `boop-acp`, `boop-harness`, `boop-mux`, `boop-proc`,
`boop-store`, `boop-turnstrip`, `boop-turnvis`.

Tooling: `ast-grep` is not installed on this machine. The sweeps ran through
`sprefa-extract` (`extract --family call`, 51,414 call facts) with the call
site spans resolved back to their source literals.

## Contents

- [Why](#why)
- [Sweep receipts](#sweep-receipts)
- [CLI usage ranking](#cli-usage-ranking)
- [The relational core](#the-relational-core)
- [Schemaless surface](#schemaless-surface)
- [The five yaml files](#the-five-yaml-files)
- [Open](#open)

## Why

`boop --help` is 422 lines / 27,011 bytes / ~6,753 tokens, emitted from one
`format!` at `crates/boop/src/cli/mod.rs:25`. Bare `boop` exits 2 with
`Error: a command or --preset is required`.

The decision taken: redesign the subcommand and relational model against a
written contract before touching the help text. That contract is a new
`boop-types` crate holding five yaml documents and nothing else.

## Sweep receipts

| sweep | finding |
| --- | --- |
| env | 48 literal names, 344 call sites. `var` 56, `var_os` 46, `set_var` 28, `remove_var` 13, `vars` 1 |
| sql | 510 call sites in 23 files. 305 of them in `crates/boop-store/src/ident.rs`. 118 distinct table names. SELECT 146 / INSERT 99 / PRAGMA 49 / DROP 26 / UPDATE 21 / DELETE 21 / CREATE TABLE 16 / WITH 9 / ALTER 5 |
| os | spawn 323 (`Command::new` 302), fs-mutate 1,068, fs-read 133, `process::exit` 11, `set_current_dir` 8, `current_dir` 14 |
| net | 42 call sites, 18 distinct. UnixStream/UnixListener 12, Tcp 9, tungstenite 6, `url::Url::parse` 10, ureq 1 |
| cli | 4 clap structs (`Cli`, `FactArgs`, `QueryArgs`, `UsageArgs`), 32 clap enums, all declared in `crates/boop/src/main.rs` |

### env names, by prefix

`BOOP_*` 36, plus `CARGO_TARGET_DIR`, `CODEX_HOME`, `HOME`, `LANG`, `LC_ALL`,
`LLMOCK_BIN`, `PATH`, `PI_CODING_AGENT_DIR`, `TMUX_PANE`, `TMUX_TMPDIR`,
`XDG_CACHE_HOME`.

Several names the doctrine documents did not surface in this sweep
(`BOOP_IDLE_SHUTDOWN_SECS`, `BOOP_DISK_FLOOR_GB`, `BOOP_COMMIT_QUIET_SECS`,
`BOOP_DOOR_WINDOW_SECS`, `BOOP_DOOR_COOLDOWN_SECS`, `BOOP_NO_SYNC`). They are
read through a helper rather than a direct `std::env::var`. `env.yaml` needs a
second pass over that helper before it is closed.

### fs-mutate is test-weighted

1,068 mutation sites across 72 files, led by `remove_file` 297 and
`remove_dir_all` 293. Both counts are dominated by test teardown, not
production paths. Split production from `#[cfg(test)]` before this number
means anything.

## CLI usage ranking

`agent_cmd` holds 221,528 rows; 2,963 of them have `program = boop`. Parsed as
URL paths, those 2,963 rows expand to 3,724 invocations over 87 distinct paths.
The subcommand tree has 112 paths.

80% of traffic is 8 paths:

| n | % | cum | path |
| --- | --- | --- | --- |
| 648 | 17.4 | 17 | `/db "<sql>"` |
| 969 | 26.0 | 43 | `/beep <route> <body>` |
| 407 | 10.9 | 54 | `/beep/lane/create` |
| 331 | 8.9 | 63 | `/beep/lane/list` |
| 178 | 4.8 | 68 | `/wait` |
| 157 | 4.2 | 72 | `/beep/lane` (no subcommand given) |
| 124 | 3.3 | 76 | `/ --help` |
| 118 | 3.2 | 79 | `/beep/ps` |

33 of 112 paths logged zero calls: every `/tag/*` verb, `/beep/lane/revive`,
`/beep/agent/subscribe`, `/beep/agent/unsubscribe`, `/beep/fork/*`,
`/beep/selection/*`, `/db/favorite/{show,edit,delete}`, `/db/skill/*`,
`/db/turn/*`, `/db/fetch/*`, `/me/mood`.

Flags carry the weight `lane create` does not: `--brief` 388, `--branch` 377,
`--base-sha` 256, `--goal` 241, `--preset` 208, `--model` 132.

`--help` ran 124 times at ~6,753 tokens each, ~838k tokens.

### This ranking is already a column

`agent_trace_event.detail` stores a fixed six-field object per invocation:

```json
{"command":"shell-init","outcome":"ok","options":[],
 "harness":null,"lane":null,"duration_ms":22}
```

The ranking above re-derived by hand what
`GROUP BY json_extract(detail,'$.command')` answers directly. Reconcile the two
before `cli.yaml` is written; the argline parse and the trace event may
disagree on what counts as one invocation.

## The relational core

70 tables. 29 are `dict_*` interning tables.

`session_id` is the most-referenced entity: a column on **17 of 70 tables**,
with 129 SQL references to `dict_session` alone.

| table | carries `session_id` |
| --- | --- |
| `agent_cmd` `agent_fetch` `agent_live` `agent_live_span` `agent_pr` | yes |
| `agent_route` `agent_session` `agent_session_attr` `agent_skill` `agent_span` | yes |
| `agent_touch` `agent_trace_event` `agent_trace_span` `agent_turn` | yes |
| `agent_turn_comment_target` `agent_usage` `sync_cursor` | yes |

Store size at measurement: 7,887 sessions, 1,020,624 turns, 2.1 GB on disk.

### The interning pattern

```
dict_session
  id  value
   1  44e793dd-2739-4dbf-b4fe-db2d2265b9f9/agent-ac6bf553135586491   60 bytes
   2  44e793dd-2739-4dbf-b4fe-db2d2265b9f9                          36 bytes
```

Write trace for one turn:

| step | value |
| --- | --- |
| 1 | caller hands in `"44e793dd-…/agent-ac6bf553135586491"` |
| 2 | `INSERT OR IGNORE INTO dict_session(value)` — UNIQUE makes it idempotent |
| 3 | `SELECT id FROM dict_session WHERE value=?` → `1` |
| 4 | `INSERT INTO agent_turn(session_id, …) VALUES (1, …)` |
| 5 | read path joins back: `JOIN dict_session d ON d.id=t.session_id` |

Two constraints do the work:

- `INTEGER PRIMARY KEY` aliases sqlite's rowid, so lookup by id is the B-tree
  key itself with no secondary index.
- `TEXT NOT NULL UNIQUE` is the reverse index, and is why step 2 can be blind.

| | text stored everywhere | interned |
| --- | --- | --- |
| `agent_turn.session_id` | ~57 MB | ~2 MB |
| join predicate | 60-byte `memcmp` | integer compare |
| rename one session | UPDATE across 17 tables | UPDATE 1 row |

The same shape covers `dict_cwd`, `dict_harness`, `dict_role`, `dict_branch`,
`dict_trace`, and 24 more.

## Schemaless surface

Two candidates, both fixed shapes in a generic costume:

| hatch | table.column | what is actually in it |
| --- | --- | --- |
| EAV | `agent_session_attr(session_id, key_id, value, set_ts)` | 2 distinct keys ever: `effort` 83 rows, `reset_ts` 1 row |
| JSON | `agent_trace_event.detail` | 287/300 sampled rows; the same 6 fields every row |

Count of genuinely schemaless storage: zero. Both collapse into declared
columns.

The write path does share one property with a document store: step 2 of the
interning trace is `INSERT OR IGNORE` on a value never declared in advance, so
new session ids, cwds and branches appear without a migration.

## The five yaml files

`crates/boop-types/` holds yaml and nothing else. No Rust behavior.

| file | holds |
| --- | --- |
| `cli.yaml` | reverse description of every clap struct and enum in `crates/boop/src/main.rs`: 4 structs, 32 enums, 112 subcommand paths, every flag with its type and default |
| `relational.yaml` | the 70 tables as flat OpenAPI schemas, `dict_*` expressed as `x-boop-id` |
| `sql.yaml` | the 510 call sites grouped by statement verb and table |
| `env.yaml` | the 48 names with read/write op, default, and the files that touch each |
| `crates.yaml` | the 8 crates, their dependency edges, and which boundary each owns |

### First projection: the most-referenced block

```yaml
Session:
  x-boop-table: agent_session
  x-boop-id: dict_session          # INTEGER PRIMARY KEY <-> TEXT UNIQUE
  x-boop-referenced-by:            # 17 tables carry session_id
    [agent_cmd, agent_fetch, agent_live, agent_live_span, agent_pr,
     agent_route, agent_session_attr, agent_skill, agent_span, agent_touch,
     agent_trace_event, agent_trace_span, agent_turn,
     agent_turn_comment_target, agent_usage, sync_cursor]
  type: object
  required: [session_id, harness_id]
  properties:
    session_id: { type: integer, x-dict: dict_session }
    harness_id: { type: integer, x-dict: dict_harness }
    nickname:   { type: string, nullable: true }
    cwd_id:     { type: integer, x-dict: dict_cwd, nullable: true }
    branch_id:  { type: integer, x-dict: dict_branch, nullable: true }
    started_ts: { type: integer, format: epoch-ms, nullable: true }
```

Source DDL, verbatim from the store at schema version 33:

```sql
CREATE TABLE dict_session (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE agent_session (
  session_id INTEGER PRIMARY KEY,
  harness_id INTEGER NOT NULL,
  nickname   TEXT,
  cwd_id     INTEGER,
  branch_id  INTEGER,
  started_ts INTEGER
);
```

## Open

| question | why it is open |
| --- | --- |
| env helper | 6 documented `BOOP_*` names never appear as a direct `std::env::var`; the helper that reads them is unlocated |
| fs-mutate split | 1,068 sites is production and test teardown mixed; the production number is unknown |
| `agent_cmd` vs `agent_trace_event` | two records of the same invocation, not yet reconciled |
| 118 table names vs 70 tables | the sql sweep's table regex catches aliases and CTE names; needs a join against `sqlite_master` |
| dead paths | 33 zero-call paths measured over `agent_cmd` history only, which starts at an unknown date |
