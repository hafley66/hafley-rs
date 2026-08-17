# Agent pipe over resident chats: DL6-ready, not DL6-dependent

Date: 2026-08-14. Status: plan, not built. Supersedes the one-shot concatMap
harness design for the next iteration; the running shell harness
(`instant/plans/2026-08-14-concatmap.md`) stays as the v0 probe.

No DL6 engine is used to build this. The constraint is that everything built
here must be expressible as DL6 relations later: policy is data (facts), the
engine is an evaluator over facts returning actions, and state is a relation
file. When DL6 lands, the same pipeline runs by replacing the internal
fact/action representation with DL6 facts and rules — no structural rework.

## TOC

1. Goal
2. Why DL6 rules own the behavior
3. Architecture
4. DL6 surface (type signatures)
5. Effect interpretation
6. Instance lifetimes
7. Storage, read/write order, uniqueness
8. Runtime order for one pair
9. Boop seams this depends on
10. Facts learned from the v0 harness
11. Tests
12. Open decisions

## Goal

One resident pipeline per source session: read visible turns from the boop
store, drive a resident interactive opencode (flash4) chat through tmux
send-keys, accumulate outputs as DL6 appends/retractions into one state file,
auto-commit that file for historical analysis, and render the state to d2 or
mermaid. The behavior of the pipe — which agent answers which request, with
which template, reminder cadence, retraction policy — is declared as hosted
DL6 rules, not shell code.

The declaration the user writes reads like:

```dl6
% this agent, when this is requested, behaves like this
rel agent(tighten).
rel on_request(agent: text, request: text, action: text).

rule on_request(tighten, "rewrite", "send(template=tighten, remind=2/8)").
rule on_request(tighten, "stale_pair", "skip").
```

## Why policy is data (and stays DL6-shaped)

The v0 harness hardcodes policy in bash: which pairs map (SQL filter),
coalesce cap 4, reminder never, template fixed per experiment dir. Every
policy change is a script edit and a resident restart. With behaviors as
facts and rules:

| policy | v0 (bash) | hosted DL6 |
| --- | --- | --- |
| which requests map | hand-written WHERE clause | `on_request` rule |
| bundling (zip N turns) | fixed pair | rule-derived bundle size |
| reminder cadence | absent | `remind(every=2, cap=8)` fact |
| stale pair policy | coalesce in shell | `on_request(_, "stale_pair", _)` rule |
| output shape | prose rewrite | appends/retractions against state relations |
| session linkage | none | edge written at spawn |

The pipe engine becomes an evaluator plus an effect interpreter; new
experiments are new rule files, no rebuild.

## Architecture

```mermaid
flowchart TD
    Store["boop store<br>agent_turn facts"] -->|"turn query<br>delta since cursor"| Bundle["bundle<br>zip N turns, rule-chosen"]
    Bundle --> Gate["DL6 evaluate<br>on_request rules"] -->|"actions"| Effects
    Sub Effects["effect interpreter"]
    Effects -->|"tmux send-keys -l"| Chat["resident opencode TUI<br>flash4, one persistent session"]
    Effects -->|"remind / skip"| Chat
    Chat -->|"transcript jsonl<br>byte-offset tail"| Detect["reply closed"]
    Detect --> Fold["fold reply into state<br>DL6 assert / retract"]
    Fold --> StateFile["one .dl6 state file<br>auto git commit per fold"]
    StateFile --> Render["iterate to d2 / mermaid"]
    Store -->|"sync ingest"| Ledger["mapper session rows<br>+ spawn edge"]
```

Operator spelling (rxjs -> this circuit):

| rx | here |
| --- | --- |
| source | turn query over the store |
| `zip`/`bufferCount` | bundling, size from rules |
| `switchScan` | fold with cancel-on-new-pair (matches coalesce-to-newest) |
| `expand` | reply deltas recursively refine state relations |

## DL6-ready surface (type signatures)

V1 implements these as plain Rust structs loaded from a rule file (json or
toml). The shapes below are the DL6 spelling they must map onto one-to-one
when the engine lands; final grammar settles against the sprefa DL6 work
(`ARCH.pl`, enum planes, `key(n)` declarations).

```dl6
% ingest facts (asserted by the pipe from boop rows)
rel turn(session: text, turn: int, role: text, said: text, ts: int).
rel pair(session: text, turn: int, ai_text: text, user_text: text).
rel stale(session: text, turn: int).

% agent behavior declarations (user-authored)
rel agent(id: text).
rel on_request(agent: text, request: text, action: text).
rel policy(agent: text, remind_every: int, remind_cap: int, bundle: int).

% state the agent maintains (appends/retractions fold here)
rel state_note(agent: text, key: text, body: text).
rel state_edge(agent: text, from: text, to: text, kind: text).
```

Rust side, the evaluator boundary:

```rust
pub trait Dl6Host {
    /// Facts the pipe asserts before evaluation.
    fn assert(&mut self, fact: Fact) -> Result<()>;
    /// Evaluate rules; return actions, not side effects.
    fn evaluate(&mut self) -> Result<Vec<Action>>;
}

pub enum Action {
    Send { template: String, vars: BTreeMap<String, String> },
    Remind { text: String },
    Skip,
    Assert(Fact),
    Retract(Fact),
    Commit { path: PathBuf, note: String },
}
```

## Effect interpretation

The evaluator never touches tmux, files, or git. One interpreter owns every
side effect:

| action | interpreter call |
| --- | --- |
| `Send` | `Multiplexer::send` literal keys into the mapper pane + Enter |
| `Remind` | same channel, template `reminder` |
| `Skip` | drop the pair, advance cursor |
| `Assert`/`Retract` | apply to the state file, then `Commit` |
| `Commit` | `git add <state file> && git commit` in the pipe worktree |

## Instance lifetimes

| instance | lifetime |
| --- | --- |
| pipe process (rust, replaces concatmap-loop.sh) | resident, one per source session, tmux window |
| resident opencode TUI | same window as the pipe's target pane; outlives every pair; dies with the window |
| `Dl6Host` | owned by the pipe; state reloaded from the state file at boot |
| `Multiplexer` handle | one `&dyn` for the process; per-call socket, existing convention |
| transcript tailer | per mapper session; byte cursor in memory, seeded from file length at boot |

## Storage, read/write order, uniqueness

- State: one `<worktree>/state/<agent>.dl6` file, git-tracked. Appends and
  retractions applied by the interpreter; the commit history is the analysis
  corpus.
- Cursor: `<worktree>/state/cursor`, monotone max ts, as in v0.
- Order per fold: read transcript delta -> evaluate (assert facts) ->
  interpret actions (mutate state file) -> commit -> advance cursor. State
  file changes always follow a completed reply, never interleave with an
  in-flight send.
- Uniqueness: one pipe per (source session, agent); mapper session id is
  assigned at spawn and written to the spawn edge; pairs dedupe on
  (session, turn) as in v0.

## Runtime order for one pair

```
step 0  turn query delta        cursor -> new pair fact
step 1  evaluate on_request     -> [Send(template, vars)]
step 2  interpreter sends       tmux send-keys -l into opencode pane
step 3  tail mapper transcript  wait for assistant turn to close (timeout guard)
step 4  evaluate reply facts    -> [Assert/Retract...]
step 5  apply + commit state    one git commit per fold
step 6  advance cursor          next pair; chat context carries in-session
```

## Boop seams this depends on

| seam | state | plan reference |
| --- | --- | --- |
| push on new turns (no polling) | missing; poll today | `crates/soopy/plans/2026-08-14-git-optional-watch.md` (DirectoryWatcher) |
| transcript tail with byte cursor | exists | `boop::tail::read_complete_lines` |
| tmux literal send + capture | exists | `boop-mux` `Multiplexer` |
| session spawn edge to mapper | `sync_session(parent=...)` writes it | one persistent session, set at spawn |
| `run_follow` re-discovery | missing (sessions discovered once) | fix or replace with DirectoryWatcher-fed sync |
| turn-grain pipe state in db | impossible (read-only SQL) | state stays in worktree files, as v0 |

## Facts learned from the v0 harness

| fact | consequence |
| --- | --- |
| one-shot flash4 over a full turn: minutes per pass, cap 3 | persistent chat removes per-pair session boot; latency budget still rules out mergeScan |
| stored `said` can be double-encoded (leading `"`) | any string filter must `trim(char(34))` first; rust side should validate on read |
| mapper's own prompts re-enter the turn query | filter by spawn-edge relationship, not text matching, once edges exist |
| coalesce-to-newest fired for real (17 stale pairs dropped) | keep as switchScan cancel semantics |
| instant open action drops into tmux targets | resident TUI pane is directly watchable; no new UI surface needed |

## Tests

| case | input | expected | why |
| --- | --- | --- | --- |
| rules route a request | `on_request(tighten,"rewrite","send(...)"))` + one pair | exactly one Send action | core dispatch |
| stale pair rule | second pair arrives mid-flight | cancel in-flight, newest proceeds | switchScan semantics |
| reminder cadence | 4 pairs, remind_every=2 cap=2 | reminders on pairs 2 and 4 only, then silent | budget is a fact, not code |
| fold idempotence | replay same transcript delta | no second commit | cursor + dedupe |
| retract path | reply contains retraction of existing state key | state file loses the key, commit notes it | appends alone cannot correct state |
| send fidelity | template text with quotes, newlines, backticks | pane receives byte-identical text | literal send-keys, no shell interpolation |
| timeout guard | mapper never closes turn | pair marked stale, pipe survives, no zombie send | deadlock guard |
| spawn edge | pipe spawns mapper session | `agent_edge(parent, mapper, "spawned")` row exists | boop relation between piped sessions |

Untested: model output quality (template tuning, not harness); multi-source
fan-in (declared out of scope below).

## Open decisions

| decision | options | lean |
| --- | --- | --- |
| DL6 dependency | none for v1; fact/action structs shaped as relations | swap to sprefa DL6 crate when it lands, no pipeline change |
| crate home for the pipe | `crates/concatmap` in hafley-rs vs instant-side | hafley-rs (session note 2026-08-14) |
| reply parse | transcript jsonl roles vs pane text | transcript jsonl |
| state file format | pure DL6 facts vs facts + fenced render output | pure facts; render is derived |

## Out of scope

- Multi-source sessions into one mapper (one pipe per source session).
- instant UI changes; the existing open-into-tmux action is the watch surface.
- Model choice policy in rules (flash4 fixed for v1; `policy` relation can
  grow a model column later).
