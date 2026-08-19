# Mood

One attribute row on a session: names the text format agents mail that session in.

| verb | what it does |
| --- | --- |
| `boop me mood <name>` | set the caller's session mood |
| `boop me mood` | print the caller's effective mood |
| `boop me mood --clear` | delete the caller's own mood row |
| `boop me` | prints `mood: <name> (set by <session>)` after registering |
| `boop beep lane create --mood <name>` | set the spawned child lane's mood |
| `boop db "select * from agent_session_attr"` | the rows, as always |

`boop me mood` also takes `--as <session>` to name the session explicitly when the caller cannot be resolved from the environment.

A template names any of four placeholders: `{from}`, `{id}`, `{body}`, `{kind}`.

| mood | template |
| --- | --- |
| plain | `[boop {id} from {from}] {body}` |
| unga | `unga: lists/tables/mermaid only, no prose\n{from} -> {id}\n{body}` |
| board | `\| {from} \| {id} \| {body} \|` |

- effective mood is a session's own `mood` attribute row; with none, the nearest ancestor's, walking `agent_lane.parent_lane_id` toward the root; with none on that chain, `plain`
- resolution is one recursive CTE, never one query per ancestor
- mail renders through the RECEIVER's effective mood, never the sender's

## One hail, two moods

```
before (plain): [boop m-4f2 from coordinator] lane fix/tls done rc=0

after (unga):
unga: lists/tables/mermaid only, no prose
coordinator -> m-4f2
lane fix/tls done rc=0
```
