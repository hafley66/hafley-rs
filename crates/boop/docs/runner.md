# boop run

`boop run resident-coroutine.dl6 --session ses_source --resident-model <model>` compiles the DL6 program, starts its UDS harness, and keeps one resident chat channel serially answering the program's `resident_ask` rows.

`--goal` accepts text or `@path`; `--poll` accepts `s` and `ms` durations and defaults to `5s`; `--name` selects `~/.agent/run/<name>/`.

| Input | Runner action | Output |
| --- | --- | --- |
| `agent_turn` for `--session` | `POST /arrive` to `turn` | program derives `resident_ask` |
| added `resident_ask` | send its prompt through the chat channel in `user_run` order | `POST /arrive` to `resident` |
| existing `resident(session, user_run)` | skip that ask | restart does not resend it |

The runner reads `/rel/resident_ask/deltas?since=<tick>` when available. Engines without that route are read through `/rel/resident_ask`, with answered rows filtered locally.

| Candidate | UDS support | selected |
| --- | --- | --- |
| `hyper` with `hyperlocal` | yes | |
| `ureq` | no | |
| `reqwest` unix-socket feature | yes | |
| `curl` | yes | yes, synchronous UDS client with the smallest added Rust dependency set |
