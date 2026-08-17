# Traitify the multiplexer seam: instant adopts boop-mux

Date: 2026-08-15.

## TOC

1. Goal
2. Why now
3. Current state
4. Gap analysis
5. Typed model
6. Trait extension
7. Pty-attach fork
8. Dependency wiring
9. Object-safety and fakes
10. Migration steps
11. Risks
12. Decisions open

## Goal

Move instant off its nine ad-hoc `tmux` shell-outs in `src-tauri/src/pty.rs`
onto `boop-mux`'s `Multiplexer` trait, and add a typed session/window/pane model
to `boop-mux` so both `boop` and `instant` describe tmux state with structs, not
tab-joined `-F` strings. One trait, one `Tmux` implementation, two consumers.

## Why now

Two consumers already need the same surface, and instant is rebuilding it by
hand. instant's flat `Session` struct and `-F` parsing duplicate what `boop` asks
of tmux through the trait. There is also a live selection bug in instant (tmux
copy-mode highlight drifts one row, direction flips per restart) that traces to
instant managing tmux sizing and mouse mode through raw shell-outs with no
single source of truth; a typed model plus a trait-owned resize path gives the
drift a single place to be fixed instead of per-call-site guessing.

## Current state

instant drives tmux nine ways, all through `tmux_cmd()` (`-u`, `-L instant-prod`
in prod) or a raw `CommandBuilder`, all output hand-parsed from `-F` strings:

| op | command | parsed into |
| --- | --- | --- |
| list sessions | `list-sessions -F "#{session_name}\t#{session_windows}\t#{session_attached}\t#{session_activity}\t#{session_created}"` | `Session` (pty.rs:55) |
| pane info | `list-panes -a -F "#{session_name}\t#{pane_current_path}\t#{pane_current_command}"` | `paths` / `commands` maps |
| attach-or-create | `new-session -A -D -s <name> [-c cwd] [cmd]` as the pty child (pty.rs:383) | none |
| has session | `has-session -t =name` | exit status |
| mouse | `set-option -t <name> mouse on` | none |
| clipboard | `set-option -g set-clipboard on` | none |
| kill | `kill-session -t <name>` | none |
| scroll | `copy-mode -e -t <name>` + `send-keys -X -N <n> scroll-up\|down` | none |
| rogue ttys | `list-panes -a -F "#{pane_tty}"` | `HashSet<String>` |

`boop-mux` exposes one trait with 15 object-safe methods: `session_of_pane`,
`pane_pid`, `live_sessions`, `has_session`, `kill_session`, `target_alive`,
`capture_pane`, `new_detached_session`, `new_bare_session`, `send_keys_literal`,
`send_text`, `send_key_named`, `new_window`, `swap_windows`, `kill_window`, plus
`ControlClient` (`tmux -C`), `parse_event`, `Notification`/`ControlEvent`.

`boop` is the only consumer. It re-exports the trait and binds a `&'static dyn
Multiplexer` (`boop/src/tmux.rs:4`), and carries a `FakeMux` test double
(`boop/src/test_support.rs:31`) implementing the full trait.

## Gap analysis

| instant needs | boop-mux today | verdict |
| --- | --- | --- |
| rich `list-sessions` (windows/attached/activity/created) | `live_sessions` returns names only | add typed method |
| `list-panes -a` (cwd + command per pane) | `session_of_pane`/`pane_pid`, single pane | add typed method |
| `has-session =name` | `has_session` (exact-target) | already covered |
| `kill-session` | `kill_session` | already covered |
| `new-session -A -D` inside the pty | `new_detached_session` (`-d`) | architectural, see Pty-attach fork |
| `set-option` (mouse, clipboard) | none | add |
| `copy-mode -e` + `send-keys -X scroll` | `send_keys` only, no copy-mode | add |
| `list-panes #{pane_tty}` | none | add |
| `-u` UTF-8 on every client | no `-u` anywhere in the impl | add to `Tmux` command builder |
| socket isolation `-L instant-prod` | `socket: Option<&str>` per call | already covered |

## Typed model

Add these structs to `boop-mux` (a new `model` module, re-exported from
`lib.rs`). Field names mirror tmux's own format variables so a reader can map
each struct field to the `#{...}` token that populated it.

```rust
pub struct Session {
    pub name: String,
    pub windows: u32,      // #{session_windows}
    pub attached: bool,    // #{session_attached} != "0"
    pub activity: i64,     // #{session_activity}, ms or s; order-only
    pub created: i64,      // #{session_created}
    pub paths: Vec<String>,    // distinct pane cwds
    pub commands: Vec<String>, // distinct foreground commands
}

pub struct Pane {
    pub session: String,        // #{session_name}
    pub target: String,         // "#{session_name}:#{window_index}.#{pane_index}"
    pub tty: String,            // bare device name, no /dev/
    pub pid: Option<u32>,       // #{pane_pid}
    pub current_path: String,   // #{pane_current_path}
    pub current_command: String,// #{pane_current_command}
}

pub struct Window {
    pub session: String,
    pub index: usize,
    pub name: String,           // #{window_name}
    pub active: bool,
}
```

`instant`'s `Session` (pty.rs:55) and the `paths`/`commands` maps collapse into
`Session`; the rogue-ttys `HashSet` collapses into a query over `Pane::tty`.
`LiveSessions { names }` stays for the cheap liveness path; it is not extended,
so the existing `boop` call sites do not churn.

## Trait extension

Six new methods, all object-safe (`&self`, `socket: Option<&str>`), keeping the
existing return-style split (`Option` for reachability, `anyhow::Result` for
fallible ops). The `-u` flag is folded into every `Command::new("tmux")` in the
`Tmux` impl, matching instant's `tmux_cmd()`.

```rust
// list sessions + per-session pane rollups in one listing
fn live_sessions_detailed(&self, socket: Option<&str>) -> Option<Vec<Session>>;

// all panes across all sessions, for cwd/command/tty rollups
fn list_panes(&self, socket: Option<&str>) -> Option<Vec<Pane>>;

// set a per-session option (mouse on)
fn set_option(&self, socket: Option<&str>, session: &str, option: &str, value: &str) -> Result<()>;

// set a server-wide option (set-clipboard on)
fn set_global_option(&self, socket: Option<&str>, option: &str, value: &str) -> Result<()>;

// copy-mode scroll: enter copy-mode then scroll N lines
fn copy_mode_scroll(&self, socket: Option<&str>, session: &str, up: bool, lines: u32) -> Result<()>;
```

`set_option` and `set_global_option` are one trait method each, not a merged
`set(scope, ...)`, because tmux's `-t` (target) and `-g` (global) flags are
distinct argv shapes and merging them forces a scope enum that adds nothing.

## Pty-attach fork

instant's pty **is** the tmux client. `open_session` spawns
`tmux -u [-L instant-prod] new-session -A -D -s <name> [-c cwd] [cmd]` as the
pty child via `portable_pty` (pty.rs:383). `-A` attach-or-create and `-D`
detach-others are client behavior, not management ops; `boop` never does this
(it spawns detached sessions and talks to them through `ControlClient`).

Do not put the pty spawn inside `Multiplexer`. Keep `portable_pty` in instant.
Expose one argv helper so instant stops duplicating the socket/env/UTF-8 logic:

```rust
// returns the argv + env instant feeds to portable_pty::CommandBuilder
pub fn attach_or_create_argv(
    socket: Option<&str>,
    name: &str,
    cwd: Option<&str>,
    command: Option<&str>,
) -> (Vec<String>, Vec<(String, String)>);
```

This is a free function on `boop-mux`, not a trait method, because it has no
fallible tmux interaction; it only builds argv. instant's `open_session` becomes
a thin caller: build argv, openpty, spawn, keep the reader thread. The pty
ownership, `PtyStore`, and the reader pump stay in instant untouched.

## Dependency wiring

`boop-mux` is a workspace member of `hafley-rs` (`edition.workspace`,
`license.workspace`, `tracing.workspace`), not published to crates.io. instant
cannot pull it by version yet. Two options:

| option | cost | notes |
| --- | --- | --- |
| path dependency | none | `boop-mux = { path = "../hafley-rs/crates/boop-mux" }`; couples instant build to a sibling checkout |
| publish to crates.io | one `cargo publish` + version bump | decouples; `edition.workspace`/`tracing.workspace` must resolve for a standalone publish |

Default: publish to crates.io, because instant already ships a release build
and should not depend on a sibling directory on the build machine. If publish
is deferred, use the path dependency and mark the version switch as a follow-up.

## Object-safety and fakes

The trait is consumed as `&dyn Multiplexer` (`boop/src/runtime.rs:205`) and has a
`FakeMux` (`boop/src/test_support.rs:31`). Every new method must stay
object-safe (`&self`, no generic type parameters) and `FakeMux` must gain the six
new methods. `attach_or_create_argv` is a free function and imposes no fake
burden.

## Migration steps

Ordered so instant never regresses mid-migration.

1. Add the `model` module (`Session`, `Pane`, `Window`) to `boop-mux`; unit-test
   the `-F` string parsing with captured `tmux` output.
2. Add `-u` to every `Command::new("tmux")` in the `Tmux` impl.
3. Add the six trait methods to `Multiplexer` + `Tmux` impl, with tests against a
   throwaway `TestServer` (the existing pattern in `boop-mux` tests).
4. Implement the six methods on `FakeMux` in `boop`; keep `boop` green.
5. Add `attach_or_create_argv` + a test asserting the prod vs dev argv.
6. Wire instant to `boop-mux` (publish or path dep per Decisions open).
7. Replace instant's `list_sessions_blocking` + `session_pane_info` with
   `live_sessions_detailed`; delete the `-F` parsing.
8. Replace `enable_mouse`'s two `set-option` calls with `set_option` +
   `set_global_option`; keep the retry loop in instant (it is instant's timing
   concern, not the trait's).
9. Replace `scroll_session` with `copy_mode_scroll`.
10. Replace `tmux_ttys` with a `list_panes` query over `Pane::tty`.
11. Replace `has-session` and `kill-session` call sites with the existing
    `has_session`/`kill_session` methods.
12. Replace the `new-session -A -D` CommandBuilder with `attach_or_create_argv`.
13. Delete `tmux_cmd()` and the now-unused `-F` constants from `pty.rs`.

Each step lands with `cargo check` + the `boop-mux` test suite green; instant's
selection/resize behavior is re-checked by hand after step 8 (mouse) and step 9
(scroll), since those touch the tmux state that feeds the copy-mode drift.

## Risks

- **`-u` change is global.** Folding `-u` into every command is correct for both
  consumers (both already want UTF-8) but touches every `boop` call path; a
  regression here is silent underscore-for-wide-char, so step 2 needs a test that
  asserts a non-ASCII session name round-trips.
- **`Session` field semantics differ across tmux versions.** `session_activity`/
  `session_created` are ms on newer tmux and s on older (pty.rs already notes
  this). The typed model keeps them as `i64` order-only, same as today.
- **The copy-mode drift may not be fixed by this alone.** The one-row selection
  flip is a sizing/race problem; traitifying gives it one `resize`/`copy_mode`
  path to instrument but does not, by itself, remove the race. Track it
  separately from this refactor.
- **`FakeMux` drift.** Every new trait method must land on the fake in the same
  change or `boop`'s tests stop compiling.

## Decisions open

| decision | options | default |
| --- | --- | --- |
| dependency mode | publish to crates.io, path dependency | publish |
| `live_sessions_detailed` return | `Option<Vec<Session>>` (None = unreachable), `Result<Vec<Session>>` | `Option` to match `live_sessions` |
| pty-attach argv helper | free function, trait method | free function |
| `Window` in the model | include now, defer until a consumer needs it | defer |
| `-u` flag home | fold into `Tmux` impl, leave per-call | fold into impl |
