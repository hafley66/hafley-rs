# boop-acp-official-crate

Replace boop's opencode channel with an ACP channel built on the official
`agent-client-protocol` crate. PR #38 (closed) hand-rolled JSON-RPC; never
again.

## Base
- repo `~/projects/hafley-rs`, worktree branch `fix/boop-acp-official-crate`
- first action: `git merge --ff-only 2f6871d` (origin/main). Failure = STOP.

## Own
- `crates/boop/src/channel/acp.rs` (new), `crates/boop/src/channel.rs`
  (module wiring only), `crates/boop/src/channel/opencode.rs`,
  `crates/boop/Cargo.toml`, `docs/failure-modes.md` (one new entry).
- Forbidden: `channel/tui.rs`, `channel/jsonrpc.rs`, `harness/*`, `main.rs`,
  store schema. If a change there is unavoidable, STOP and report the line.

## Libraries (decided, no research needed)
- `agent-client-protocol` 2.0.0 (crates.io, repo agentclientprotocol/rust-sdk)
- `agent-client-protocol-tokio` 0.11.1 for the stdio transport helpers
- tokio (boop has none today; a current-thread runtime owned by the channel,
  since `LaneChannel` is a sync trait; do not convert the trait to async)
- reference client: rust-sdk `src/agent-client-protocol/examples/yolo_one_shot_client.rs`
- skill: `~/projects/claude-research/skills/acp/SKILL.md` (methods, update kinds, stop reasons)

## Shape
- `AcpChannel::open(spec: &ChannelSpec, command: &[String]) -> Result<AcpChannel>`
  spawns the agent process, `initialize`, `session/new` with ABSOLUTE cwd
  (kimi rejects relative), then if `spec.model` is Some:
  `session/set_config_option {configId:"model", value}` (opencode ignores
  opencode.json and OPENCODE_MODEL under ACP and hangs silently on its dead
  default endpoint `opencode/big-pickle`, lab receipt
  `~/projects/labs/acp-lab/README.md`).
- `impl LaneChannel for AcpChannel`: `start_turn` = `session/prompt`;
  `next_event` drains `session/update` notifications and maps the prompt
  result `stopReason`: `end_turn` -> `TurnEvent::ok`, `cancelled|refusal|
  max_tokens|max_turn_requests` -> `failed`, JSON-RPC error -> `flaked` with
  the error message verbatim; `steer` -> `Delivery::NextTurn`; `interrupt` ->
  `session/cancel`; `close` kills the child.
- `session/request_permission` -> auto-select the first allow option
  (lanes run unattended). `fs/*` and `terminal/*` requests: return a
  JSON-RPC method-not-found; do not implement.
- `conversation_id` = ACP sessionId; `conversation_id_kind` = "acp_session".
- `OpencodeChannel::open` becomes `AcpChannel::open(spec, ["opencode","acp"])`;
  delete the `opencode run` + transcript-scrape path in opencode.rs
  (`last_message_state`, `newest_session`, `newest_activity`) only if nothing
  outside the file references them (grep first; if referenced, leave and
  report).

## Gates (run all, paste output)
- `cargo test -p boop` green; `cargo clippy -p boop -- -D warnings`
- live: `boop beep lane create --dry-run` unchanged; then a real turn through
  the new channel against `opencode acp` with model
  `openrouter/deepseek/deepseek-v4-flash-0731`: the prompt "reply with the
  single word pong" returns `end_turn` under 30s. Paste the trail.
- fail-first: a unit test that feeds a canned `session/prompt` error frame
  and asserts `TurnEvent::flaked`, red before the mapping, green after.
- no `eprintln!` in `src/**`; tracing only.

## Report
`TASKS/boop-acp-official-crate.REPORT.md` in the worktree: files, gate
outputs, commit shas, the PR url. Commit, push, open the PR. Do not merge.
