# Agent control buy lab

Date: 2026-08-21. This folder adds no workspace dependency. Every executable
receipt used provider-free local fixtures or protocol-only commands, an
isolated `/private/tmp` home/cache/target, and zero provider prompts.

## Decision

| Decision question | Evidence-backed result |
| --- | --- |
| ACP session owner now | `acpx@0.13.1` passed a provider-free persistent named-session receipt under approved local execution: two prompts, history, permission response, and `cancel -> stopReason=cancelled`. The workspace sandbox rejects the queue Unix socket with `EPERM`; that is an execution restriction, not a product failure. Boop's direct Rust `AcpChannel` remains the lane owner for current harness adapters because this lab found no Rust ACPX embedding API. |
| Task/mail/receipt vocabulary | `a2a-lf@0.3.0` provides Rust `Task`, `Message`, `Part`, and `Artifact` vocabulary for networked task/reply/artifact correlation. OACP `0.4.3` provides file-message fields `id`, `conversation_id`, `parent_message_id`, sender and recipient. Boop's local acknowledgement timestamp, exit-code receipt, route, trace, worktree, and liveness fields remain in its own types. |
| Machinery above ACPX | `agent-team@0.2.0` passed a provider-free fake `codex-acp` receipt: two named workers, a second prompt on one worker, `allow` and `deny`, a parallel worker prompt while another was held, and a cancellation request. Its package carries a 4.4 MiB darwin-arm64 binary. The measured workers used local ACP stdio and UDS control, without tmux or PTY scraping. |
| Relational observation | Boop retains the SQLite projection of transcripts, agent traces/spans, lane purposes, routes, worktree coordinates, mailbox acknowledgement rows, process/tmux liveness, and cross-harness identity. The candidate protocols do not provide this joined local observation surface. |

## Re-run

The reusable ACP probe needs Node 22+ and `acpx` on `PATH`. It creates its ACP
agent itself and sends no provider request.

```sh
mkdir -p /private/tmp/hafley-rs-agent-control-buy-deps/acpx
npm pack acpx@0.13.1 --ignore-scripts --pack-destination /private/tmp/hafley-rs-agent-control-buy-deps/acpx
npm install --prefix /private/tmp/hafley-rs-agent-control-buy-deps/acpx/site --ignore-scripts /private/tmp/hafley-rs-agent-control-buy-deps/acpx/acpx-0.13.1.tgz
ACPX_BIN=/private/tmp/hafley-rs-agent-control-buy-deps/acpx/site/node_modules/.bin/acpx \
  node crates/boop/labs/0_agent_control_buy/0_probe.mjs
```

Receipt from the installed `acpx@0.13.1`:

```json
{"probe":"acpx-fake-acp-exec","provider_calls":0,"cold_ms":160.4,"direct_stdio":true,"permission_policy":"approve-all","tmux_or_pty":false}
```

`cold_ms` is included by the driver and intentionally varies by host. The
receipt uses ACPX `exec`, which starts the fake agent directly. ACPX persistent
`prompt -s` uses a temporary Unix queue socket. The workspace sandbox rejects
that bind with `EPERM`. The following approved local execution proves the queue
path separately; it still sends no provider request:

```sh
ACPX_BIN=/private/tmp/hafley-rs-agent-control-buy-deps/acpx/site/node_modules/.bin/acpx \
  ACPX_PERSISTENT=1 node crates/boop/labs/0_agent_control_buy/0_probe.mjs
```

```json
{"probe":"acpx-fake-acp-persistent","provider_calls":0,"cold_ms":246.9,"direct_stdio":true,"permission_policy":"approve-all","tmux_or_pty":false,"named_session":true,"persistence":true,"cancellation":"cancelled"}
```

```sh
npm pack agent-team --ignore-scripts --pack-destination /private/tmp/hafley-rs-agent-control-buy-deps/agent-team
npm install --prefix /private/tmp/hafley-rs-agent-control-buy-deps/agent-team/site --ignore-scripts agent-team@0.2.0
node /private/tmp/hafley-rs-agent-control-buy-deps/agent-team/site/node_modules/agent-team/bin/agent-team.js --help
```

Observed command surface: `add`, `rm`, `ls`, `ask`, `log`, `cancel`, `allow`,
`deny`, `info`, `restart`, `mode`, and `set`. `ls` returned `No agents running`.
The candidate adapter list is closed, but its `codex` adapter resolves the
`codex-acp` executable on `PATH`. The reusable probe writes that executable as
a provider-free local fake ACP adapter. This approved local execution covers
the live worker registry:

```sh
ACPX_BIN=/private/tmp/hafley-rs-agent-control-buy-deps/acpx/site/node_modules/.bin/acpx \
AGENT_TEAM_BIN=/private/tmp/hafley-rs-agent-control-buy-deps/agent-team/site/node_modules/agent-team/bin/agent-team.js \
AGENT_TEAM=1 node crates/boop/labs/0_agent_control_buy/0_probe.mjs
```

The following is the emitted `agent_team` field; the enclosing receipt also
contains the direct ACPX preflight fields.

```json
{"provider_calls":0,"named_workers":["alpha","beta"],"idle_rss_kib":{"alpha":6096,"beta":6240},"second_prompt_persistence":true,"permission":"allow-and-deny","concurrent_workers":"beta completed while alpha hold was queued","cancellation":"request-sent"}
```

```sh
python3 -m pip download --no-deps --dest /private/tmp/hafley-rs-agent-control-buy-deps/oacp oacp-cli==0.4.3
python3 -m pip download --dest /private/tmp/hafley-rs-agent-control-buy-deps/oacp pyyaml==6.0.3
python3 -m pip install --no-index --find-links /private/tmp/hafley-rs-agent-control-buy-deps/oacp --target /private/tmp/hafley-rs-agent-control-buy-deps/oacp/site oacp-cli==0.4.3 pyyaml==6.0.3
OACP_HOME=/private/tmp/hafley-rs-agent-control-buy-oacp-home PYTHONPATH=/private/tmp/hafley-rs-agent-control-buy-deps/oacp/site python3 -m oacp.cli init boop-lab --agents coordinator,worker
OACP_HOME=/private/tmp/hafley-rs-agent-control-buy-oacp-home PYTHONPATH=/private/tmp/hafley-rs-agent-control-buy-deps/oacp/site python3 -m oacp.cli send boop-lab --from coordinator --to worker --type task_request --subject probe --body 'no provider invoked'
```

Measured OACP receipt: the command wrote one YAML envelope to both
`agents/coordinator/outbox/` and `agents/worker/inbox/`; `oacp inbox --json`
reported `message_count: 1` for `worker`. OACP has no resident daemon, so idle
RSS is `n/a`.

A second local receipt sent a `follow_up` from `worker` with `--in-reply-to`
the generated task ID. The copied reply envelope retained
`conversation_id: conv-20260821-coordinator-1` and
`parent_message_id: msg-20260822004040-coordinator-6020`. `init --repo ...
--link fixture.txt:receipt.txt` also created
`artifacts/receipt.txt -> <repo>/fixture.txt`; artifact transaction semantics
beyond that workspace link remain unverified.

```sh
oacp_lab=$(mktemp -d /private/tmp/boop-oacp-adversarial-XXXXXX)
mkdir -p "$oacp_lab/repo"; touch "$oacp_lab/repo/fixture.txt"
OACP_HOME="$oacp_lab/home" PYTHONPATH=/private/tmp/hafley-rs-agent-control-buy-deps/oacp/site \
  python3 -m oacp.cli init probe --agents coordinator,worker --repo "$oacp_lab/repo" --link fixture.txt:receipt.txt
first=$(OACP_HOME="$oacp_lab/home" PYTHONPATH=/private/tmp/hafley-rs-agent-control-buy-deps/oacp/site python3 -m oacp.cli send probe --from coordinator --to worker --type task_request --subject first --body local --conversation-id conv-20260821-coordinator-1 --json)
first_id=$(printf '%s' "$first" | jq -r '.message_id')
OACP_HOME="$oacp_lab/home" PYTHONPATH=/private/tmp/hafley-rs-agent-control-buy-deps/oacp/site \
  python3 -m oacp.cli send probe --from worker --to coordinator --type follow_up --subject reply --body local --in-reply-to "$first_id" --json
```

```sh
git init /private/tmp/hafley-rs-agent-control-buy-deps/a2a-rs
git -C /private/tmp/hafley-rs-agent-control-buy-deps/a2a-rs remote add origin https://github.com/a2aproject/a2a-rs.git
git -C /private/tmp/hafley-rs-agent-control-buy-deps/a2a-rs fetch --depth 1 origin 9d70cfd6bca55f8b3801be9d64f036f2ac9d4c42
git -C /private/tmp/hafley-rs-agent-control-buy-deps/a2a-rs checkout --detach FETCH_HEAD
CARGO_HOME=/private/tmp/hafley-rs-agent-control-buy-deps/cargo CARGO_TARGET_DIR=/private/tmp/hafley-rs-agent-control-buy-deps/a2a-target cargo build --manifest-path /private/tmp/hafley-rs-agent-control-buy-deps/a2a-rs/Cargo.toml -p examples --bin helloworld-server --bin helloworld-client --quiet
/private/tmp/hafley-rs-agent-control-buy-deps/a2a-target/debug/helloworld-server > /private/tmp/hafley-rs-agent-control-buy-a2a-server.log 2>&1 &
pid=$!; sleep 1; ps -o pid=,rss=,command= -p "$pid"
/private/tmp/hafley-rs-agent-control-buy-deps/a2a-target/debug/helloworld-client
kill "$pid"; wait "$pid" || true
```

At source commit `9d70cfd6bca55f8b3801be9d64f036f2ac9d4c42`, the local official
hello-world server measured `9776 KiB` RSS. Its client completed `send_message`,
`get_task`, `cancel_task`, `send_streaming_message`, and
`subscribe_to_task (cancel)` across gRPC, JSON-RPC, and HTTP+JSON. The agent
card declares streaming and `pushNotifications: false`; push notification
configuration exists in the SDK/CLI but is unverified against a local receiver.

## Evidence matrix

`M` means measured locally, `S` means source/package-confirmed, and `U` means
unverified in this lab. A dash denotes a surface absent from the candidate's
documented layer.

| Surface | ACPX 0.13.1 | agent-team 0.2.0 | A2A Rust SDK | OACP CLI 0.4.3 |
| --- | --- | --- | --- | --- |
| Version, license, footprint | M: MIT, 5 direct npm deps, 2292 KiB | M: MIT, optional platform binary, 4524 KiB | M: Apache-2.0, `a2a-lf` 0.3.0 has 6 direct deps | M: Apache-2.0, PyYAML >=6, 2796 KiB installed target |
| Cold invocation | M: direct `exec` driver `cold_ms`; U: isolated queue-start time | M: fake worker `add`/start executed; U: worker-only cold time | M: build and local server start | M: CLI init/send |
| Idle RSS daemon | U: no idle queue-owner RSS sample | M: alpha 6096 KiB, beta 6240 KiB provider-free workers | M: 9776 KiB hello-world server | M: n/a, no daemon |
| Second prompt persistence | M: named queue first/second/history receipt under approved execution | M: alpha first/second fake ACP receipt | S: task/context APIs | U: durable-file storage measured, no second-send/restart receipt |
| Direct named worker | M: named session `probe` | M: `alpha`, `beta` fake ACP workers | S: agent card endpoint plus task ID | M: `--to worker` |
| Concurrent workers | U: overlapping named-session prompts untested | M: beta completed while alpha hold was queued | S: independent task IDs | S: independent inbox files |
| Cancellation | M: `stopReason=cancelled` | M: command accepted (`Cancel sent`); U: terminal stop reason not surfaced by CLI | M: `cancel_task` | - |
| Permission handling | M: `session/request_permission` auto-approved | M: pending request then `allow` and `deny` | U | - |
| Task/reply correlation | M: persistent named session/history | M: worker-scoped output history | M: task IDs; S: context/message/artifact identifiers | M: reply retained `conversation_id` and `parent_message_id` |
| Artifacts | U | U: output history only confirmed | S: `Artifact`; U: hello-world has no artifact | M: init artifact symlink; U: artifact protocol transaction |
| Push/completion | S: structured turn events | S: UDS session notifications | M: streaming; U: local push receiver | S: `watch` delta CLI |
| Model selection | S: `--model` | S: `set <name> model` | S: opaque endpoint implementation | - |
| Rust embedding/API | U: no Rust API identified; JS CLI/runtime exports observed | U: no published Rust library found | M: core/client/server crates | - |
| tmux/PTTY scraping | M: false | M: neither string in platform binary | M: false | M: false |

## Candidate sources and exclusions

* ACPX package: `https://github.com/openclaw/acpx`, package `0.13.1` local
  manifest. It exports CLI, `runtime`, and `flows`; Rust embedding is unverified.
* agent-team: `https://github.com/nekocode/agent-team` and
  `ARCHITECTURE.md`. The package is an npm launcher for a platform binary.
* A2A: `https://github.com/a2aproject/a2a-rs` at the commit above. Core
  `a2a-lf` has `serde`, `serde_json`, `chrono`, `base64`, `thiserror`, and
  `uuid`; `a2a-client-lf` and `a2a-server-lf` are separate Rust crates.
* OACP: `https://github.com/kiloloop/oacp`, local PyPI wheel `0.4.3`.
* Jockey GUI: excluded from execution. No CLI/Rust agent-control protocol
  surface was captured.
* AWS CAO: excluded from execution. Its documented default backend is tmux
  and its supervisor-worker control uses MCP.
* CAP: excluded from execution. Its public draft describes PTY as its universal
  fallback.

## Boop change map

| Adoption boundary | Delete or replace after an adapter exists | Retain |
| --- | --- | --- |
| ACPX foreground coordinator | `crates/boop/src/cli/acpx.rs`; `crates/boop/src/cli/mod.rs` ACPX module declaration; `crates/boop/src/cli/mail.rs` ACPX branch at `deliver_hail` | `crates/boop-acp/src/channel.rs`, `crates/boop-acp/src/channel/acp.rs`, `crates/boop-proc/src/supervise.rs`, and the route/mailbox projection |
| agent-team worker controller | A replacement adapter would supersede the ACPX foreground branch above | `LaneChannel`, Boop lane supervisor, route/worktree data, and store projections |
| A2A task/artifact layer | No current file is directly deletable: map A2A IDs into a new adapter before altering `boop-store::bus::Message` | `crates/boop-store/src/bus.rs` `Message` and `Route`; its local `to_timestamp`, `rc`, `detail`, and worktree-route fields have no A2A field mapping |
| OACP mailbox transport | No current file is directly deletable: a translator must preserve route and completion fields | `crates/boop-store/src/bus.rs`; `crates/boop/src/cli/mail.rs`; SQLite observation and lane completion rows |

The existing single-crate Boop ACP implementation is direct Rust ACP:
`crates/boop-acp/src/channel/acp.rs`. The current ACPX CLI wrapper is only the
foreground coordinator path. This is the boundary used for the first decision.
