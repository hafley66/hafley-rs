# boop help sweep

Documentation and clap-metadata only. No flag, no arg-parsing semantic, no
behavior changed. Branch `docs/boop-help-sweep`, cut at `172ee58`.

## Contents

1. [What changed, per page](#what-changed-per-page)
2. [Stale claims killed](#stale-claims-killed)
3. [The new DELIVERY table](#the-new-delivery-table)
4. [Behavior findings, not fixed](#behavior-findings-not-fixed)
5. [Validation](#validation)
6. [Worktree state](#worktree-state)

## What changed, per page

Every top-level about line now fits under 100 chars and names who calls the
verb. Every page that an agent can act on carries a verified `EXAMPLES` block,
in `after_help` so the newlines survive clap's paragraph collapsing.

| page | before | after |
|---|---|---|
| `boop --help` | DOCTRINE claimed pane injection "mid-turn or idle", named four per-harness mid-turn transports | DOCTRINE opens with SHAPE (six crates, two verb trees, two stores) then a five-arm DELIVERY table and an ACP block, each cited |
| `boop beep` | "Drive agents: harnesses, lanes, mail, processes" | "Drive agents: spawn lanes, hail them, register coordinators, measure them" + long_about splitting change-verbs from read-verbs |
| `boop beep hail` | "Type into a running agent, and say whether the keystrokes landed"; 5 bare args | "Send one message to a running agent, and say how it was delivered" + the four-arm summary + 2 examples; 0 bare args |
| `boop beep lane` | "Lanes: the agents boop spawns and tracks" | names the six things you can do; long_about says `lane create` is the only spawn door |
| `boop beep lane create` | "Make a worktree, spawn the agent, register the route" | "The spawn door: cut a worktree, start the agent, register the route" + the branch-name derivation + 2 examples |
| `boop beep lane run` | "Drive one lane conversation" | says it runs over ACP, 700 ms mailbox poll, and holds every hail for a resume turn |
| `boop beep lane wait` | "Wait for the lane's result row" | adds that a result row is append-only and nothing pushes it |
| `boop beep agent` | "Register pane-less coordinators and native subagents" | states outright that a `native` row has no transport behind it |
| `boop db` | run-on about; the `usage` child leaked a clap implementation note | short about, long_about names the two forms and says this store is NOT the mailbox; `usage` about rewritten; 3 examples |
| `boop debug` | 2-line run-on about | short about + long_about + 2 examples |
| `boop agent` | "Freshly synchronize and summarize Boop agent/runtime/activity facts" | says it is a read verb and points spawn/register at `boop beep agent`; 2 examples |
| `boop tell-parent` / `tell-children` | run-on about | short about + who calls it + 3 examples total |
| `boop whoami` | "Report the caller's own identity and the rung that resolved it" | adds the ladder order and what the printed name is for |
| `boop wait` | run-on about | says why a block is the universal pull; 2 examples |
| `boop inbox` | "Mail a claude coordinator reads at a turn boundary" | says installing the hooks FLIPS that name from push to pull; 2 examples |
| `boop me` | "Register this Codex pane…" with `--name` unqualified | states the CODEX-ONLY constraint (`me.rs:107` requires a root Codex transcript, stamps `harness: codex`) and points other harnesses at `boop adopt`; 3 examples |
| `boop config` | "Inspect the boop configuration the CLI reads" | names the three outputs; 2 examples |
| `boop adopt` (hidden) | "…every other harness has it typed into its pane" | says `--harness claude` flips push to pull and `--no-hooks` keeps injection |
| `boop beep harness list/get` | no about at all, positional with no help | both have about lines and the positional has help |

Argument help, counted by a scan for `#[arg(...)]` with no preceding `///`:

| | before | after |
|---|---|---|
| bare `#[arg(...)]` across `crates/boop*/src/**` | 156 | 0 |
| undocumented positionals in `main.rs` | 15 | 0 |

## Stale claims killed

Every row was re-verified against the code in this worktree before the text was
written.

| killed claim | where it was | current truth | cite |
|---|---|---|---|
| "A parent whose route is kind=coordinator … gets that hail TYPED INTO ITS PANE, mid-turn or idle; no wait needs arming" | DOCTRINE COMPLETION | false twice. A `result` row never reaches `deliver_hail` at all, and a claude coordinator with the drain hook is a PULL | `crates/boop-proc/src/supervise.rs:830-845` (append only), `crates/boop/src/cli/mail.rs:174-188` |
| "Reaches a running lane MID-TURN on all four harnesses" + a four-row per-harness transport table (claude stdin stream-json, codex app-server steer, opencode TUI Enter, kimi TUI C-s) | DOCTRINE HAIL | no surviving path delivers mid-turn. All four harnesses open `AcpChannel`; `steer` returns `Delivery::NextTurn` unconditionally | `crates/boop-acp/src/channel/acp.rs:177-180`; `claude.rs:76`, `codex.rs:45`, `kimi.rs:41`, `boop-acp/src/channel/opencode.rs:37` |
| "A harness with no in-flight port would report `nextturn` … none does today" | DOCTRINE HAIL | inverted. Every harness reports `nextturn` today; none reports `midturn` | `crates/boop-acp/src/channel/acp.rs:177-180` |
| "edge kind deliver-midturn/deliver-nextturn" as if both occur | DOCTRINE HAIL | only `deliver-nextturn` is reachable from production code. `Delivery::MidTurn` is produced solely by test fakes at `supervise.rs:1340` and `:1390` | `crates/boop-proc/src/supervise.rs:664-686`, `:1053-1066` |
| "the cross-harness agent-event reader, 1-1 with `bus` … routes to layers 0-3" | `main.rs:1-4` module doc | pre-split framing. Six crates now: boop-store, boop-harness, boop-acp, boop-mux, boop-proc, boop | `crates/*/Cargo.toml` |
| "`coordinator` makes hail deliver by pane injection" | `main.rs` inline comment at the `Adopt` arm | only when no drain hook is installed; the hook check runs first | `crates/boop/src/cli/mail.rs:174-188` |
| "about 18 s over 1.5 GB here" | DOCTRINE STORE SCHEMA | a number with no source that rots. Deleted | n/a |
| "still run as hidden aliases for one release" | DOCTRINE tail | a promise with no deadline behind it. Now reads "still parse as hidden aliases" | `crates/boop/src/main.rs` `#[command(hide = true)]` arms |
| `boop wait 01J8XYZ...` as an example | `wait` doc | the parser rejects that shape. Replaced with `boop wait <message-id>`, and a test now pins every example to the parser | `crates/boop/src/main.rs` `every_help_example_parses` |
| `usage`: "clap needs both attributes to accept the two forms" | `db usage` about | an implementation note in user-facing help. Replaced with what the verb does and what `--show-sql` is for | n/a |

`bus dispatch` appears nowhere in `crates/**` or `docs/**`; nothing to kill.
Verified by grep over `--include=*.rs --include=*.md`.

Two facts the old help never stated and now does:

| new statement | cite |
|---|---|
| a `native` row is addressable and has NO transport: an Agent-tool child has no route, no pane, no stdin, no ACP session id, so every hail to it takes the "no pane" arm | `crates/boop/src/cli/job.rs:1085-1130` (writes `tmux: None`), `crates/boop/src/cli/mail.rs:192-195` |
| a lane only receives kinds `request\|hail\|note\|retry\|resume`; any other `--kind` sits in the file | `crates/boop-proc/src/supervise.rs` `deliverable/1` |
| the drain-hook check also reads `~/.claude/settings.json`, so a user-level install flips every matching name to pull | `crates/boop-proc/src/inbox.rs` `installed_for/2` |
| a claude model REFUSES a tmux lane spawn without an explicit `--harness claude` | `crates/boop-proc/src/lane.rs:367` |

## The new DELIVERY table

Rendered by `boop --help`, cited to `crates/boop/src/cli/mail.rs:152-211`, whose
four early returns are the whole dispatch.

| # | X's route | transport | X sees it |
|---|---|---|---|
| 1 | no registry route | none, row stays queued | never |
| 2 | `kind=lane` | X's own supervisor reads the mailbox, 700 ms poll | next turn |
| 3 | drain hook installed in the route's cwd or in `~/.claude/settings.json` | none pushed; X's own `Stop` / `UserPromptSubmit` hook runs `boop inbox drain` | next turn boundary |
| 4 | live pane, no drain hook | tmux paste-buffer + Enter into the pane | at once, at the keyboard |
| 5 | `kind=coordinator\|native`, no live pane | none, row stays queued | never |

Plus: a `result` row takes no arm at all, and only `boop wait`,
`boop beep lane wait`, `boop beep lane create --wait` or a turn-boundary drain
ever reads one.

## Behavior findings, not fixed

| # | finding | cite | why it matters |
|---|---|---|---|
| 1 | **A default hail records no store edge.** `run_hail` defaults `kind` to `"request"`, and `record_control_edge` only writes an edge for `hail\|result\|retry\|resume\|cancel`. `request` is not in that list, so `boop beep hail <lane> --body "..."` with no `--kind` writes zero `agent_edge` rows | `crates/boop/src/cli/mail.rs:143` (default), `:461-463` (filter) | "did X get it" through `agent_edge` is blind for the common case. The `deliver-nextturn` edge the supervisor writes still lands, so the gap is the SEND side only |
| 2 | **`boop db "<sql>"` takes 30-53 s.** The startup sync (`command_needs_startup_sync`) runs before any `db` read. `SELECT 1`, idle tree, debug build: 37079, 34572, 53048 ms. Same query against the INSTALLED release binary `~/.cargo/bin/boop`: 36986, 29556, 29273 ms. Not a debug-profile artifact. Nine documented read-only examples all landed in the 32-53 s band | `crates/boop/src/main.rs` `command_needs_startup_sync/1`, `sync_before_local_command/1` | a 10-second-law defect on the most-documented read verb, and it is already shipped |
| 3 | **`TuiChannel` is dead code.** It is constructed only in its own `#[cfg(test)]` block; nothing in `src/**` opens it. It is the only type that ever returned `Delivery::MidTurn` | `crates/boop-acp/src/channel/tui.rs:45`, constructor call sites all at `:696-838` | it keeps the `MidTurn` arm alive in the type, which is exactly what made the old DOCTRINE readable as true |
| 4 | pre-existing clippy warning, untouched: `needless_borrow` at `crates/boop/tests/host_chat.rs:44` | that line | outside the files this sweep owns; `cargo clippy -p boop --all-targets` still exits 0 |

## Validation

Run from `~/projects/hafley-rs-worktrees/boop-help-sweep`.

| command | result |
|---|---|
| `cargo build -q -p boop` | rc 0 |
| `boop --help` | renders, rc 0 |
| `boop <sub> --help` for beep, db, debug, agent, concatmap, host, tell-parent, tell-children, whoami, wait, inbox, me, config | 13/13 rc 0 |
| `cargo test -q -p boop` | rc 0, see the per-binary table below |
| `cargo clippy -q -p boop --all-targets` | rc 0, one pre-existing warning |

Test totals, `cargo test -p boop`:

| | count |
|---|---|
| test binaries | 19 |
| passed | 123 |
| failed | 0 |
| ignored | 1 |

The `bin boop` binary is 49 of those, and includes two new tests this sweep
adds:

| test | what it pins |
|---|---|
| `every_help_example_parses` | every `boop ...` line the help prints is accepted by the parser. FAIL-PRE-FIX on `boop wait 01J8XYZ...` |
| `every_help_example_is_printed_somewhere` | the example list cannot drift from the pages: each entry must appear in some page's long help |

Examples run for real, wall time measured:

| example | rc | ms |
|---|---|---|
| `boop config path` | 0 | 129 |
| `boop config presets` | 0 | 127 |
| `boop db "SELECT name FROM sqlite_master WHERE type='table'"` | 0 | 53091 |
| `boop db turn list --limit 5 --format text` | 0 | 36017 |
| `boop db status --window 10` | 0 | 36053 |
| `boop debug --since 10m` | 0 | 37351 |
| `boop debug --lane fix-help-sweep --json` | 0 | 39337 |
| `boop agent summary --format text` | 0 | 32714 |
| `boop agent sessions --cwd ~/projects/hafley-rs` | 0 | 50817 |

Every ms figure above is the startup sync, see finding 2.

The mutating examples (`tell-parent`, `tell-children`, `beep hail`,
`beep agent register|done`, `inbox hooks|drain`, `me`, `concatmap`) are covered
by `every_help_example_parses` rather than run against the live mailbox. The
concatmap set was additionally driven to a post-parse filesystem error
(`--template /nonexistent.md` returns "No such file or directory"), which proves
clap accepted the shape.

## Worktree state

`~/projects/hafley-rs-worktrees/boop-help-sweep`, branch `docs/boop-help-sweep`,
pushed to `origin`. No PR opened.

| | |
|---|---|
| content commit | `4924b49 docs(boop): make --help the delivery contract it claims to be` |
| trailing commit | this file's own state block |
| `git status --short` | empty, tree clean |
