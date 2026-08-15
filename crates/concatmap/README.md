# concatmap

A resident agent pipe: watch another AI chat session, and for each new message
pair, drive a second, cheaper AI chat to react to it — with the behavior
declared in a small rule file instead of code.

The name is the RxJS operator. Each incoming message pair maps to one pass
through the inner chat, one at a time, in order, newest wins:

```
source   --pair1----pair2----pair3-->
concatMap --[==pass1==][==pass2==]-->   pair3 waits; a newer pair cancels a stale pass
```

## What it does

You point it at a source session (any transcript already in the boop store)
and a tmux pane running an interactive opencode chat. For every new
(user message, assistant reply) pair in the source:

1. Read the pair from the boop store.
2. Ask the rule file what to do with it (`send` with a template, or `skip`).
3. Type the rendered prompt into the opencode pane via tmux send-keys.
4. Wait for the reply to land in the opencode transcript.
5. Fold the reply into the agent's state file as appends and retractions.
6. Commit the state file. The git history is the run's record.

The inner chat is a real, attachable opencode session. `tmux attach` to the
pane and you are watching the pipe work, live, in the model's own UI.

## The rule file

Behavior lives in one toml file. This is the whole API you author against:

```toml
agent = "tighten"

[[route]]
request = "rewrite"
action = "send(template=tighten, remind=2/8)"

[[route]]
request = "stale_pair"
action = "skip"

[policy]
remind_every = 2   # inject a reminder every 2 pairs
remind_cap = 8     # at most 8 reminders total, then silence
bundle = 1         # pairs per pass
```

- `request` is a classification of the pair (`rewrite` for a normal pair,
  `stale_pair` when a newer pair already superseded it).
- `action = "send(template=X, remind=A/B)"` sends the pair through template X
  with a reminder budget; `action = "skip"` drops it.
- A typo in an action is a hard error. Misrouting fails loudly, never
  silently.

Change the rule file, not the code, to change behavior.

## Run it

One pass per invocation; a shell loop (or a watcher, later) drives it:

```sh
concatmap \
  --agent tighten \
  --session <source-session-id> \
  --pane <tmux pane running opencode> \
  --worktree <dir holding state/> \
  --rules tighten.toml \
  --transcript <opencode session jsonl>
```

State layout under `--worktree`:

```
state/
  tighten.dl6   # the agent's accumulated state, committed after every fold
  cursor        # last source turn seen; restarts resume here
```

## Layout

| module | role |
| --- | --- |
| `fact.rs` | the vocabulary: turns, pairs, routes, policies, state notes |
| `rules.rs` | loads and dispatches the rule file; classifies pairs into requests |
| `host.rs` | evaluates rules into `Action`s; owns no side effects |
| `action.rs` | `Send`, `Remind`, `Skip`, `Assert`, `Retract`, `Commit` |
| `interp.rs` | the only module allowed to touch tmux, files, or git |
| `pipe.rs` | turn query, pair bundling, transcript tailing, the pass loop |
| `state.rs` | the state file: apply, fold replies, render, save |
| `cursor.rs` | the monotone source cursor |

The evaluator/interpreter split is the point: `host` decides, `interp` does.
Everything that touches the world goes through one module, which is what makes
the behavior testable and the side effects auditable.

## DL6-ready, not DL6-dependent

The rule file is a stand-in for DL6, a datalog-ish relation language. Every
struct in `fact.rs` and `rules.rs` maps one-to-one onto a planned relation
(`on_request(agent, request, action)`, `policy(agent, remind_every,
remind_cap, bundle)`), and the state file is already written in that fact
syntax. When the DL6 engine lands, the rule file becomes DL6 rules and the
pipeline does not change shape.

## Tests

```sh
cargo test -p concatmap
```

32 tests: routing, action parsing (including malformed actions erroring),
reminder budgets, fold idempotence, cursor monotonicity, state
apply/retract, and template rendering.

## Non-goals

- One pipe per source session; no multi-source fan-in.
- No model choice in rules yet; the inner chat's model is whatever the pane
  runs.
- No push-based wakeups yet; v1 is poll-per-invocation until the watcher seam
  lands.
