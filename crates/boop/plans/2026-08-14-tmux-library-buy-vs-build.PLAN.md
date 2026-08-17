# tmux talk: buy vs build: 2026-08-14

User ask, verbatim intent: "is there a rust tmux library? research buy vs build for
how we talk to tmux: shell is fine, but seeing raw shell in test code for tmux
when we have a tui trait with a tmux impl means we are going outside the
abstraction."

## TOC

1. [Current state](#1-current-state)
2. [Raw call-site inventory](#2-raw-call-site-inventory)
3. [Candidates](#3-candidates)
4. [Verdict table](#4-verdict-table)
5. [Recommendation](#5-recommendation)
6. [Migration list (candidate 4)](#6-migration-list-candidate-4)
7. [Open items](#7-open-items)

## 1. Current state

| piece | file:line | what it is |
|---|---|---|
| `Multiplexer` trait | `crates/boop-mux/src/lib.rs:21-75` | 11 methods: `session_of_pane`, `pane_pid`, `live_sessions`, `has_session`, `kill_session`, `target_alive`, `capture_pane`, `new_detached_session`, `send_keys_literal`, `send_text`, `send_key_named`, `new_window`, `swap_windows` |
| `Tmux` impl | `crates/boop-mux/src/lib.rs:79-363` | the one implementation, doc comment at :1-6 calls it "a mix of raw spawns and `tmux_interface` one-shot builders" |
| `tmux_interface` usage today | `has_session:141-159`, `kill_session:161-179`, `new_detached_session:231-261` | 3 of 13 impl methods go through the crate; the rest (`session_of_pane`, `pane_pid`, `live_sessions`, `target_alive`, `capture_pane`, `new_window`, `swap_windows`, `send_keys`) are raw `Command::new("tmux")` |
| `ControlClient` | `crates/boop-mux/src/lib.rs:444-561` | long-lived `tmux -C` child, `parse_event`/`ControlEvent`/`Notification` at :368-426, used today only inside boop-mux's own tests |
| `kill_test_server` | `crates/boop-mux/src/lib.rs:635-651` | raw spawns, exposed production helper for test lifecycle |
| dependency | `crates/boop-mux/Cargo.toml:12` | `tmux_interface = "0.4"`, only dependency besides `anyhow`/`tracing` |
| re-export seam | `crates/boop/src/tmux.rs:1-14` | thin re-export + `mux() -> &'static dyn Multiplexer`, already reachable from every test module via `crate::tmux::mux()` and `crate::tmux::kill_test_server` |

`boop-mux` is a normal (non-dev) dependency of `boop` (`crates/boop/Cargo.toml:17`),
so no wiring is needed to call the trait from test code: it is already imported.

## 2. Raw call-site inventory

`grep -rn 'Command::new("tmux")' crates/boop crates/boop-mux --include='*.rs'`
returns 23 call sites (`main.rs:4` is a comment, not counted). Split by
production vs test:

| bucket | count | files |
|---|---|---|
| production, inside `impl Multiplexer for Tmux` / `kill_test_server` | 11 | `boop-mux/src/lib.rs:83,105,123,185,206,309,347,464,613,637,644`: this IS the abstraction boundary, not a leak |
| test code, raw shell around the trait | **12** | see table below: this is the leak the user named |

Test-code raw sites, one row per call:

| file:line | op | trait method already covering it | gap? |
|---|---|---|---|
| `boop-mux/src/lib.rs:691` (`TestServer::create_session`) | `new-session -d -s <name>` (bare, no cwd/command) | none: `new_detached_session` requires `cwd` + `command` | **GAP** |
| `boop/src/main.rs:2230` (`LiveTmuxSession::new`) | `new-session -d -s <name>` (bare, default socket) | none, same gap | **GAP** |
| `boop/src/harness/opencode.rs:689` (`opencode_send_injects_into_a_live_pane`) | `new-session -d -s <name>` (bare) | none, same gap | **GAP** |
| `boop/src/harness/claude.rs:621` (`claude_send_injects_into_a_live_pane`) | `new-session -d -s <name>` (bare) | none, same gap | **GAP** |
| `boop-mux/src/lib.rs:843` (`target_alive_...` test) | `kill-session -t alive` | `kill_session` (trait:33) | migration only |
| `boop/src/main.rs:2243` (`LiveTmuxSession::drop`) | `kill-session -t <name>` | `kill_session` (trait:33) | migration only |
| `boop/src/channel/tui.rs:583` (`start_turn_respawns_a_dead_agent_window`) | `kill-window -t <target>` | none | **GAP** |
| `boop/src/harness/opencode.rs:710` | `capture-pane -t <name> -p` | `capture_pane` (trait:38-43) | migration only |
| `boop/src/channel/tui.rs:589` | `capture-pane -p -t <target>` | `capture_pane` (trait:38-43) | migration only |
| `boop/src/harness/claude.rs:642` | `capture-pane -t <name> -p` | `capture_pane` (trait:38-43) | migration only |
| `boop/src/harness/opencode.rs:722` (`has_session_on` helper) | `has-session -t <name>` (prefix match) | `has_session` (trait:31, exact `=` match) | migration, semantics tighten |
| `boop/src/harness/claude.rs:712` (`has_session_on` helper) | `has-session -t <name>` (prefix match) | `has_session` (trait:31, exact `=` match) | migration, semantics tighten |

`exact_target` doc at `boop-mux/src/lib.rs:598-600`: `-t name` prefix-matches a
sibling session (`-t boop` matches `boop-shell-v2`); `-t =name` pins exact.
The two `has_session_on` test helpers use the looser prefix form; the trait's
`has_session` is already the exact, stronger check.

**Net: 2 methods missing from the trait, 10 of 12 sites are pure migration.**

## 3. Candidates

### Candidate 1: `tmux_interface` used fully

Push every remaining production raw spawn (`session_of_pane`, `pane_pid`,
`live_sessions`, `target_alive`, `new_window`, `swap_windows`, `send_keys`)
through `tmux_interface` builders, same as `has_session`/`kill_session`/
`new_detached_session` already do.

| axis | finding |
|---|---|
| mechanics | crate wraps `Command` argv construction per tmux subcommand; still spawns one process per call, same as raw |
| maintenance | crates.io `max_version` 0.4.0, published 2026-03-10, 149,775 total downloads / 58,147 recent: actively used, single-maintainer (`AntonGepting/tmux-interface-rs`) |
| operation coverage | `ListSessions`, `ListPanes`, `DisplayMessage`, `SwapWindow`, `SendKeys`, `NewWindow`, `KillWindow`, `CapturePane`, `HasSession`, `KillSession`, `NewSession`, `AttachSession`, `SplitWindow` and more: covers every op in the table above |
| control-mode support | crate's own `control_mode` module is documented "(unimplemented, draft)": matches the existing doc comment at `lib.rs:2-3` ("no crate on crates.io sells one") |
| test-server ergonomics | unaffected either way: this candidate is about the `impl Multiplexer`, not the test layer |
| migration cost | ~8 methods, argv-shape only, no behavior change; `send_keys_literal`'s `-l --` literal-argument path needs a docs.rs read to confirm `SendKeys` exposes the literal flag (see [Open items](#7-open-items)) |
| verdict | worth doing for hygiene and consistency, but **does not touch the user's complaint**: it only changes what's behind the trait, not whether tests go through it |

### Candidate 2: other crates.io tmux crates

| crate | what it is | verdict |
|---|---|---|
| `tmux-rs` (`richardscollin/tmux-rs`) | "A Rust port of tmux": a from-scratch reimplementation of the tmux **server**, not a client/binding library; 1,985 total downloads, last published 2025-08-29 | reject: wrong shape, does not talk to a running tmux at all |
| `tmuxpulse` | "a real-time, event-driven TUI for monitoring tmux sessions" using `tmux -C`; 0.1.0, 36 downloads, published 2026-03-02 | reject: an application, not a library API; its control-mode client is the same amount of from-scratch parsing boop-mux already wrote, with 36 downloads of field use behind it |
| `tmuxrs` | tmux session manager, tmuxinator-style config replacement | reject: config/session-launcher CLI, not a driver API |
| `psmux` | Windows ConPTY multiplexer that speaks the tmux command language | reject: wrong platform, wrong direction (implements a tmux-compatible server, doesn't drive a real one) |
| `dmux-rs`, `rmuxinator` | tmux session layout tools | reject: session-launcher CLIs, not driver APIs |
| verdict | `tmux_interface` remains the only real candidate; nothing else in the search results is a client/binding library for driving tmux from Rust |

### Candidate 3: control mode (`tmux -C`) as primary transport

Expand `ControlClient` from its current test-only role into the trait's main
transport: one long-lived `tmux -C` connection instead of a process spawn per
call.

| axis | finding |
|---|---|
| mechanics | already built at `boop-mux/src/lib.rs:444-561`, hand-parsed `%begin/%end/%error` protocol, 10s deadline per command |
| maintenance | in-tree, boop owns it fully: no upstream to track |
| coverage | every op the CLI can do is expressible as one command line over the control socket; no gap vs one-shot |
| control-mode support | this candidate IS the control-mode path |
| test-server ergonomics | orthogonal: a `ControlClient`-backed `Tmux` impl still needs the SAME two new trait methods (bare session, kill-window) for tests to stop shelling out |
| migration cost | large: every `impl Multiplexer for Tmux` method rewritten to go over one connection, lifecycle/reconnect handling, socket-per-harness fan-out reconsidered |
| verdict | a real perf/architecture question (fewer process spawns), but **orthogonal to the user's ask**: closing the trait (candidate 4) is required either way before tests stop leaking raw shell; do not conflate the two |

### Candidate 4: close the trait (build side)

Add the 2 missing methods, migrate the 12 test call sites onto
`crate::tmux::mux()` / `boop_mux::Tmux`.

| axis | finding |
|---|---|
| mechanics | 2 new trait methods, both expressible via `tmux_interface` builders already in the dependency tree (`NewSession` without `.shell_command(..)` per docs.rs quick-start; `KillWindow` builder exists) |
| maintenance | zero new dependencies |
| coverage | closes all 12 leak sites: 2 by new method, 10 by calling what already exists |
| control-mode support | unaffected, independent of transport |
| test-server ergonomics | improves: test intent (`create a bare session`, `kill this window`) reads as a typed call instead of an argv literal |
| migration cost | smallest of all candidates: no architecture change, no new crate |
| verdict | **directly answers the user's stated itch** |

### Candidate 5: hybrid

Candidate 4 (mandatory, closes the leak) plus candidate 1 done incrementally
after (production raw spawns moved to `tmux_interface` builders where the
literal-`send-keys` question resolves clean). Candidate 3 stays a separate,
later decision: it is a transport question, not an abstraction-boundary one.

## 4. Verdict table

| # | candidate | closes the test-code leak? | verdict |
|---|---|---|---|
| 1 | `tmux_interface` used fully in the impl | no | do later, hygiene only |
| 2 | other crates.io tmux crates | no | reject, none is a fit (table above) |
| 3 | control mode as primary transport | no, on its own | orthogonal, defer |
| 4 | close the trait | **yes** | **recommended** |
| 5 | hybrid (4 now, 1 after, 3 deferred) | yes | recommended framing of 4 |

## 5. Recommendation

**Candidate 4: close the trait.** Add 2 methods to `Multiplexer`
(`crates/boop-mux/src/lib.rs:21-75`), implement both over `tmux_interface`
builders already in the dependency (`Cargo.toml:12`, no new crate), migrate
the 12 raw test call sites in section 2 to call them. This is the only
candidate that removes `Command::new("tmux")` from test code; the other
candidates change what happens *inside* the trait, not whether tests go
around it.

## 6. Migration list (candidate 4)

New trait methods:

| method | signature | backing builder |
|---|---|---|
| `new_bare_session` | `fn new_bare_session(&self, socket: Option<&str>, name: &str) -> Result<()>` | `tmux_interface::NewSession::new().detached().session_name(name)`, no `.shell_command(..)` call |
| `kill_window` | `fn kill_window(&self, socket: Option<&str>, target: &str) -> Result<()>` | `tmux_interface::KillWindow` |

Call sites to migrate, `raw call` -> `trait call`:

| file:line | today | after |
|---|---|---|
| `boop-mux/src/lib.rs:691` | `Command::new("tmux").args(["-L", socket, "new-session", "-d", "-s", name])` | `mux().new_bare_session(Some(socket), name)` |
| `boop/src/main.rs:2230` | same, no `-L` | `mux().new_bare_session(None, name)` |
| `boop/src/harness/opencode.rs:689` | same, with `-L` | `mux().new_bare_session(Some(&guard.socket), &name)` |
| `boop/src/harness/claude.rs:621` | same, with `-L` | `mux().new_bare_session(Some(&guard.socket), &name)` |
| `boop-mux/src/lib.rs:843` | `Command::new("tmux").args(["-L", socket, "kill-session", "-t", "alive"])` | `mux().kill_session(Some(socket), "alive")` |
| `boop/src/main.rs:2243` | same, no `-L` | `mux().kill_session(None, name)` |
| `boop/src/channel/tui.rs:583` | `Command::new("tmux").args(["-L", socket, "kill-window", "-t", target])` | `mux().kill_window(Some(socket), target)` |
| `boop/src/harness/opencode.rs:710` | `Command::new("tmux").args(["-L", socket, "capture-pane", "-t", name, "-p"])` | `mux().capture_pane(Some(socket), name, None)` |
| `boop/src/channel/tui.rs:589` | same shape | `mux().capture_pane(Some(socket), target, None)` |
| `boop/src/harness/claude.rs:642` | same shape | `mux().capture_pane(Some(socket), name, None)` |
| `boop/src/harness/opencode.rs:722` (`has_session_on`) | `Command::new("tmux").args(["-L", socket, "has-session", "-t", name])` | `mux().has_session(Some(socket), name).unwrap_or(false)`: tightens prefix match to exact |
| `boop/src/harness/claude.rs:712` (`has_session_on`) | same | same tightening |

`mux()` here means `crate::tmux::mux()` (`boop/src/tmux.rs:11-14`), already
reachable from every one of these test modules.

## 7. Open items

- `send_keys_literal` (trait:53, impl uses raw `send_keys` helper at
  `boop-mux/src/lib.rs:613-632`) claims in the module doc comment
  (`lib.rs:4-6`) that `tmux_interface` "exposes no literal-key mode." A
  docs.rs summary pass suggested `SendKeys` does carry a literal-flag builder
  method but could not confirm the exact method name from the page text
  alone: read `tmux_interface::SendKeys` source/docs directly before
  deciding whether candidate 1's `send_keys_literal` migration is possible or
  the doc comment's claim stands.
- Candidate 3 (control mode as primary transport) is a real, separate
  question (fewer process spawns per call): not scoped or estimated here;
  raise it on its own if perf on repeated tmux calls becomes a measured
  problem.
