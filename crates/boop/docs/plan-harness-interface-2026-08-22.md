# Plan: one harness interface, no harness literals

Branch `refactor/harness-interface`, worktree `hafley-rs-worktrees/harness-interface`, cut from `b012f7a`.
Goal in one sentence: a claude TUI can hail a codex or opencode TUI and vice versa through one `Harness` object, and adding a fifth harness is one `impl` plus one enum variant, with no `== "claude"` anywhere.

## Contents

1. [Leaks found](#1-leaks-found)
2. [Type signatures](#2-type-signatures)
3. [Pseudo-code](#3-pseudo-code)
4. [Instance lifetimes](#4-instance-lifetimes)
5. [Storage layout, reads/writes, uniqueness](#5-storage)
6. [Deletions](#6-deletions)
7. [Lanes and ownership](#7-lanes-and-ownership)
8. [Validation](#8-validation)

---

## 1. Leaks found

Harness identity is a `&'static str` / `String` compared against literals at 16 non-test sites. Each one is a capability the harness should declare.

| site | literal | capability it encodes |
|---|---|---|
| `boop-proc/src/lane.rs:343,360` | `"opencode"` | plan-family models banned on this harness |
| `boop-proc/src/lane.rs:365` | `"claude"` | lanes refused; workers are the coordinator's own subagents |
| `boop/src/cli/job.rs:798` | `"codex"` | `--variant` unsupported; effort rides `model@effort` |
| `boop/src/cli/me.rs:121` | `"claude"` | mail must land at a turn boundary (hook inbox), never keystrokes |
| `boop/src/cli/control.rs:44` | `"codex"` | native TUI needs a store projector |
| `boop-acp/src/channel/tui.rs:106,279,359,431` | `"opencode"` | conversation-id kind, session lookup |
| `boop-store/src/_0_session_graph.rs:464,1722` | `"codex"`, `"claude"` | process-name → harness, lane-name prefix → harness |
| `boop-harness/src/harness/claude.rs:69` | `"claude"` | own process name |
| `boop-proc/src/lane.rs` (40 literals total) | model spelling → harness table | `harness_for_model` |

Parallel shapes of the same identity: `SessionRef.harness: &'static str`, `Route.harness: Option<String>`, `HarnessSession.harness` in instant, `dict_harness.value` in SQLite. instant carries a second `HarnessStore` trait with four impls duplicating `boop-harness` discovery (`instant/src-tauri/src/0_harness_store.rs:224-536`).

Two traits with overlapping duties: `Harness` (25 methods: identity rungs, transcripts, spawn, native TUI, send) and `LaneChannel` (5 impls, two of which only carry a `TuiProfile`). `Harness::prepare_native_tui`, `send_native`, `observe_native_children`, `native_child_completion_visible` are the "drive a running TUI" concern bolted onto the transcript reader.

## 2. Type signatures

All in `crates/boop-harness`. Nothing outside the crate names a harness by string.

```rust
// harness.rs
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HarnessId { Claude, Codex, Kimi, Opencode }
impl HarnessId {
    pub const ALL: [HarnessId; 4];
    pub fn as_str(self) -> &'static str;
    pub fn parse(value: &str) -> Option<HarnessId>;
    pub fn for_model(model: &str) -> Option<HarnessId>;   // the one table; replaces lane.rs harness_for_model
}
impl std::fmt::Display for HarnessId;
impl std::str::FromStr for HarnessId;
impl rusqlite::ToSql / FromSql for HarnessId;           // dict_harness.value round-trip

/// Declared once per harness; every former literal branch reads a field here.
pub struct Capabilities {
    pub model_prefixes: &'static [&'static str],          // "claude-", "gpt-", "kimi-", "" (provider/model)
    pub bans_plan_family_models: bool,                    // lane.rs:343,360
    pub lanes: LanePolicy,                                // lane.rs:365
    pub variant: VariantSupport,                          // job.rs:798
    pub mail: MailPolicy,                                 // me.rs:121
    pub native_tui_projector: bool,                       // control.rs:44
    pub process_names: &'static [&'static str],           // session_graph.rs:464, claude.rs:69
}
pub enum LanePolicy { Allowed, CoordinatorSubagentsOnly }
pub enum VariantSupport { Flag, ModelSuffixEffort, None }
pub enum MailPolicy { Door, TurnBoundaryHook, Keystrokes }

pub trait Harness: Send + Sync {
    fn id(&self) -> HarnessId;
    fn capabilities(&self) -> &'static Capabilities;
    fn transcripts(&self) -> &dyn TranscriptSource;       // existing sessions/read_from/ingest/sync_candidates, moved
    fn live(&self) -> &dyn LiveSessions;                   // new
    fn door(&self) -> &dyn Door;                           // new
    fn spawner(&self) -> &dyn Spawner;                     // existing spawn/stop/open_channel/one_shot, moved
}

// live.rs
pub enum LiveStatus { Busy, Idle, Unknown }
pub enum DoorAddress {
    UnixSocket { path: PathBuf, token: Option<String> },   // claude: ~/.claude/sessions/<pid>.json
    AppServer { socket: PathBuf, thread: String },         // codex: remote-control daemon
    Http { base: url::Url, session: String },              // opencode: serve, port 4096
    None,                                                  // kimi TUI
}
pub struct LiveSession {
    pub harness: HarnessId,
    pub session_id: String,
    pub pid: Option<u32>,
    pub cwd: Option<PathBuf>,
    pub tmux_pane: Option<String>,                         // "%3418"
    pub status: LiveStatus,
    pub door: DoorAddress,
    pub observed_ms: u64,
}
pub trait LiveSessions: Send + Sync {
    /// Harness-native registry only; no tmux scraping, no transcript mtime.
    fn live_sessions(&self) -> Result<Vec<LiveSession>>;
    fn live_session_in_pane(&self, pane: &str) -> Result<Option<LiveSession>> { default: filter }
}

// door.rs
pub enum Delivered { Injected, QueuedForTurnBoundary, Unreachable(String) }
pub trait Door: Send + Sync {
    fn deliver(&self, session: &LiveSession, body: &str) -> Result<Delivered>;
    /// Resolves once when the session next ends a turn with nothing queued.
    fn notify_idle(&self, session: &LiveSession, timeout: Duration) -> Result<IdleNotice>;
}
pub struct IdleNotice { pub at_ms: u64, pub status_line: Option<String> }

// spawner.rs (existing methods, relocated verbatim)
pub trait Spawner: Send + Sync {
    fn spawn(&self, spec: &SpawnSpec) -> Result<SessionRef>;
    fn stop(&self, session: &SessionRef) -> Result<()>;
    fn open_channel(&self, spec: &ChannelSpec) -> Result<Box<dyn LaneChannel>>;
    fn one_shot(&self, spec: &OneShotSpec) -> Result<String> { unsupported }
    fn preview_command(&self, spec: &SpawnSpec) -> Option<String>;
}

// transcripts.rs (existing methods, relocated verbatim)
pub trait TranscriptSource: Send + Sync {
    fn sessions(&self) -> Result<Vec<SessionRef>>;
    fn session_roots(&self) -> Result<Vec<PathBuf>>;
    fn sync_candidates(&self, known: &KnownSessions) -> Result<Vec<SessionRef>>;
    fn read_from(&self, session: &SessionRef, offset: u64) -> Result<ReadChunk>;
    fn ingest(&self, store: &Store, session: &SessionRef, from: u64) -> Result<Ingested>;
    fn known_paths_can_move(&self) -> bool;
}

// registry.rs
impl Registry {
    pub fn get(&self, id: HarnessId) -> &dyn Harness;     // total, never Option
    pub fn all(&self) -> impl Iterator<Item = &dyn Harness>;
}

// boop-store/src/session.rs
pub struct SessionRef { pub harness: HarnessId, … }     // was &'static str
// boop-store/src/bus.rs
pub struct Route { pub harness: Option<HarnessId>, … }  // was Option<String>
```

## 3. Pseudo-code

```rust
// mail.rs deliver_hail, replaces five early returns + tmux send-keys arm
fn deliver_hail(registry, store, route_name, message) -> Result<Delivered> {
    // route = routes[route_name] or Unreachable("no route")
    // harness = registry.get(route.harness?)
    // live = harness.live().live_session_in_pane(route.tmux?)      // harness-native registry
    //        .or_else(|| store.agent_live row for route.session_id)  // last projection
    //        or Unreachable("no live session")
    // match harness.capabilities().mail {
    //   Door               => harness.door().deliver(&live, body)
    //   TurnBoundaryHook   => append to hook inbox (claude legacy, removed in phase 3)
    //   Keystrokes         => Unreachable("keystroke delivery retired")
    // }
    // store.record_delivery(message.id, route_name, delivered)
}

// LiveSessions for Claude
// read_dir ~/.claude/sessions/*.json; for each: parse {pid, sessionId, cwd, tmux, status, messagingSocketPath}
// skip if pid not alive; tmux "projects-2:@3418.%3418" -> pane "%3418"
// door = UnixSocket{path: messagingSocketPath, token: None}; status = busy|idle

// Door for Claude
// connect unix socket; write `{"type":"user","message":{"role":"user","content":<body>}}\n`
// Ok(QueuedForTurnBoundary) on write success; Unreachable(err) otherwise
// notify_idle: poll sessions/<pid>.json status every 500ms until Idle or timeout   (phase 1)
//              peer-protocol notify_when_idle subscription                          (phase 2)

// LiveSessions for Codex
// SELECT id, cwd, updated_at_ms FROM threads WHERE archived=0 in ~/.codex/state_5.sqlite
// pid/pane from `codex app-server` client list if exposed, else from routes table; door = AppServer{socket, thread}

// Door for Codex: `codex queue --thread <id> --message <body> --remote unix://<socket>` (existing send_native body)
// LiveSessions/Door for Opencode: GET /session ; POST /session/:id/prompt_async ; SSE /event for idle
// LiveSessions for Kimi: Ok(vec![]) ; Door: Unreachable("kimi TUI exposes no door; spawn a lane")

// harness_for_spawn(explicit, model)
// id = explicit.parse()? or HarnessId::for_model(model)? or bail
// caps = registry.get(id).capabilities()
// if caps.bans_plan_family_models && plan_harness_family(model).is_some() -> bail
// if caps.lanes == CoordinatorSubagentsOnly && explicit.is_none() -> bail
```

## 4. Instance lifetimes

| type | holds | lifetime |
|---|---|---|
| `Registry` | four `Box<dyn Harness>`, zero-sized impls | one per process, built in `main` |
| `Capabilities` | `&'static` data | `static` per harness module |
| `LiveSession` | owned strings, a `DoorAddress` | value; produced per call, projected into `agent_live`, dropped |
| `Door` impls | nothing (stateless; open a socket per `deliver`) | borrowed from the harness |
| `LaneChannel` | child process + stdio | per lane, owned by `supervise` (unchanged) |
| `IdleNotice` | two fields | returned once |

No `Arc<Mutex<_>>` introduced. The only I/O handles are opened and closed inside `deliver` / `notify_idle`.

## 5. Storage

`boop.db`, existing tables, two additions.

| table | change | written by | read by |
|---|---|---|---|
| `dict_harness` | unchanged; `HarnessId::as_str` is the value | intern | everywhere |
| `agent_live` | add `door_kind TEXT`, `door_addr TEXT`, `status_id` already present | `LiveSessions` projection pass (`boop db sync`, `boop beep` startup) | `deliver_hail`, `lane list`, instant `boop_mux_session` replacement |
| `agent_delivery` (new) | `message_id, route, harness_id, outcome, at_ms` | `deliver_hail` | `boop wait`, `boop debug` |

Sequence per hail: read route → `live_sessions()` (harness registry, no DB) → one `agent_live` upsert → `deliver` → one `agent_delivery` insert. Uniqueness: `agent_live` PK `session_id`; `agent_delivery` PK `(message_id, route)`.

Read path never scans transcripts; `LiveSessions` reads the harness's own registry file or DB.

## 6. Deletions

| gone | replaced by |
|---|---|
| `Harness::{identity_env, identity_pane, identity_process, session_id_in_pane, root_sessions_for_cwd, prepare_native_tui, send_native, observe_native_children, native_child_completion_visible}` | `live()`, `door()`, `transcripts()` |
| `boop-acp/src/channel/tui.rs` (864), `channel/opencode.rs`, `channel/kimi.rs` | `AcpChannel` for lanes; doors for TUIs |
| `boop-acp/src/channel/codex.rs::InspectingProxy` + `boop codex` proxy launch | codex `LiveSessions` reads `state_5.sqlite`; TUI launched plain with `--remote` to the daemon |
| `lane.rs::harness_for_model` + 40 literals | `HarnessId::for_model` + `Capabilities` |
| `me.rs` hook-inbox install for claude | claude `Door` (socket) |
| instant `0_harness_store.rs` four `HarnessStore` impls | instant links `boop-harness` and calls `registry.get(id).live()` / `.transcripts()` |

## 7. Lanes and ownership

Native Claude subagents (Agent tool). Disjoint files. Phase 2 starts when phase 1's crate compiles.

| lane | model | owns | delivers |
|---|---|---|---|
| **P1 harness-id** | opus | `boop-harness/src/{harness.rs,registry.rs,identity.rs}`, `boop-store/src/{session.rs,bus.rs}`, `boop-proc/src/lane.rs`, `boop/src/cli/{job,me,control}.rs`, `boop-store/src/_0_session_graph.rs`, every `impl Harness` in `boop-harness/src/harness/*.rs` (signature changes only) | `HarnessId`, `Capabilities`, trait split into `transcripts()/spawner()`, 16 literal sites rewritten through capabilities, `cargo test --workspace` green |
| **P2a doors** | sonnet high | new `boop-harness/src/{live.rs,door.rs}`, `boop-harness/src/door/{claude,codex,opencode,kimi}.rs`, `mod` lines in `boop-harness/src/lib.rs`, `live()`/`door()` bodies in `harness/*.rs` | four `LiveSessions` + four `Door` impls, each with a fixture test (claude: temp `sessions/` dir + unix socket echo; codex: temp `state_5.sqlite`; opencode: local HTTP stub; kimi: `Unreachable`) |
| **P2b mail** | sonnet high | `boop/src/cli/mail.rs`, `boop-store/src/ident.rs` (schema: `agent_live` columns, `agent_delivery`), `boop/tests/deliver_*.rs` | `deliver_hail` on `Door`, `agent_delivery` rows, `boop wait` reads them, keystroke arm removed |
| **P3 cut** | sonnet high | `boop-acp/src/channel/{tui,opencode,kimi,codex}.rs`, `boop/src/cli/acpx.rs` untouched | deletes §6 rows 2–3, `boop codex` launches without the proxy |

Every lane: no new `&str` harness comparisons (CI grep in §8), no em dashes, no banned identifiers (`provenance`, `substrate`, `load-bearing`, `regime`), commit per delivered row with receipts in the message.

## 8. Validation

| check | command | pass |
|---|---|---|
| no literal matching | `grep -rnE '== *"(claude|codex|kimi|opencode)"\|"(claude\|codex\|kimi\|opencode)" *=>' crates --include=*.rs \| grep -v tests` | 0 lines |
| workspace builds and tests | `cargo test --workspace` | green except the pre-existing `inbox_hooks::a_hail_during_a_long_turn…` |
| add-a-harness drill | a test-only `impl Harness for Echo` registered in `Registry::with(vec![...])` | `harness_for_spawn`, `deliver_hail`, `lane list` all work with no other edit |
| claude door live | `boop beep hail <claude-route> "ping"` against a real `boop tui claude` pane | message appears in that pane at its next turn boundary |
| codex door live | same against `boop tui codex` | message queued via `codex queue` |
| instant | `cargo check` in `instant/src-tauri` after it points at the new trait | compiles |
