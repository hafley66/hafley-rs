pub(crate) mod acpx;
pub(crate) mod control;
pub(crate) mod db;
pub(crate) mod debug;
pub(crate) mod job;
pub(crate) mod mail;
pub(crate) mod me;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use boop::bus::Route;
use boop::{bus, ident};

#[cfg(feature = "dl6")]
pub(crate) const CONCATMAP_EXAMPLES: &str = "\
TEMPLATE: a markdown file whose rendered form IS the prompt. Keys:
  {{mode}}      the --mode word (labels the experiment; also how the loop
                recognises and skips its own mapper prompts)
  {{ai_text}}   the assistant turn(s) before the user turn
  {{user_text}} the user turn that follows

STATE: the loop's own memory, one dir per experiment:
  <state>/cursor    last store ts seen (first run seeds at the newest ts,
                    so only pairs made AFTER launch get mapped; --from-start
                    or --cursor <N> backfills an existing conversation)
  <state>/done/     one empty marker per processed (session, turn); a restart
                    never remaps them
  For the chat feed the state dir is also the resident model's cwd.

RULES: a json file choosing the feed and bundle shape:
  {\"feed\": \"oneshot\"}                       fresh process per bundle, one model call each
  {\"feed\": \"chat\", \"goal\": \"...\"}          one enduring resident; goal is its first turn
  \"bundle\": \"pair\" (default) or \"run\"       pair = 1 ai + 1 user; run collapses same-role runs
  \"coalesce\": 0                        backlog cap; only the newest survives past it (default 0 = never drop)
  \"references\": true                   append the source session's file touches as of the bundle
  \"window\": \"SELECT ...\"               caller-owned SQL replacing the compiled bundlers:
                                         binds :session (TEXT), :session_id (INTEGER), :cursor (ms);
                                         returns INTEGER `id` + INTEGER `ts` + TEXT `text`,
                                         one row per bundle (`ts` is the cursor watermark).
                                         With a window, --template/--mode are optional (text ships
                                         verbatim) and the loop only does cursor + done + send.

WINDOW EXAMPLE (gaps-and-islands over agent_turn, same-role runs concat'ed):
  rules.json:
    {\"feed\": \"chat\",
     \"goal\": \"tighten each <ai> turn; code and numbers verbatim\",
     \"window\": \"WITH marked AS (
        SELECT t.turn, t.ts, r.value AS role, t.said,
               ROW_NUMBER() OVER (ORDER BY t.ts, t.turn)
             - ROW_NUMBER() OVER (PARTITION BY r.value ORDER BY t.ts, t.turn) AS island
        FROM agent_turn t JOIN dict_role r ON r.id = t.role_id
        JOIN dict_session s ON s.id = t.session_id
        WHERE s.value = :session AND t.ts > :cursor)
      SELECT max(turn) AS id, max(ts) AS ts, group_concat(said, char(10)) AS text
      FROM marked GROUP BY role, island ORDER BY min(ts)\"}
  boop concatmap --me --rules rules.json --state s

EXAMPLES:
  # oneshot refinement of one conversation, flash4 default model:
  boop concatmap --session ses_abc123 --mode tighten \\
    --template tighten.md \\
    --state ~/.agent/concatmap/tighten/state

  # same, but map the caller's own session (whoami ladder resolves it):
  boop concatmap --me --mode tighten --template tighten.md --state s

  # enduring resident whose history accumulates; rewrites land per turn:
  #   rules.json: {\"feed\": \"chat\", \"goal\": \"tighten each <ai> turn; code and
  #                numbers verbatim; return only the rewritten turn\",
  #                \"bundle\": \"run\", \"coalesce\": 4, \"references\": true}
  boop concatmap --me --rules rules.json --mode tighten \\
    --template tighten.md --state s

  # template file shape (tighten.md):
  #   mode: {{mode}}
  #   <ai>{{ai_text}}</ai>
  #   <user>{{user_text}}</user>";

/// The schema version is interpolated, not literal, so a bump to
/// `ident::SCHEMA_VERSION` cannot leave this text stale.
pub(crate) fn doctrine() -> String {
    format!(
        "\
DOCTRINE (this help is the usage contract; agents read it with `boop --help`):

COORDINATOR: `boop --preset codex` opens a named persistent ACPX session and
  registers it as `coordinator`. Standard input is one prompt per line. Worker
  hails enter the same ACPX queue with `--no-wait`; successful queue admission
  stamps the mailbox row. A configured model preset such as `terra` may replace
  the direct agent name. `BOOP_ACPX_BIN` selects an installed acpx executable;
  absent that, Boop runs the pinned published ACPX package.

WARMUP: after the worktree exists and before the agent starts, lane create runs
  the repo's `boop-start` just recipe if it declares one, and a repo that does
  not is skipped in silence. A FAILING recipe blocks the spawn: the pre-commit
  hook needs what it installs, and a lane that cannot commit reads the abort as
  success. `--no-start` opts out.

REGISTER: one path per kind of caller. A pane registers itself by running a
  harness TUI through Boop; a pane-less agent (a coordinator with no tmux
  session, or a native subagent) registers by name:
    boop tui <harness> [--cwd <dir>] [--name <id>]      interactive pane
    boop beep agent register <name> [--parent <id>]     pane-less route

SPAWN: every lane spawn goes through lane create; bare tmux spawns leave no
edge and stay invisible to tracking:
    boop beep lane create --branch feature/<name> --brief <abs-path> \\
      --preset <p> [--goal <text>] [--wait] [--mail-dir <d>] [--dry-run]
  ONE derivation, from the whole branch name: `feature/schema-emit` gives lane
  id and tmux session `feature-schema-emit` (`/` spelled `-`, the one character
  tmux cannot hold) and worktree `.boop-worktrees/feature/schema-emit` (the same
  name as a path). No prefix is dropped and no `lane/` prefix is added.
  Kinds are feature/fix/refactor/chore, a convention the CLI prints, not a gate.
  --cwd defaults to the repo you stand in, --base-sha to origin/main's head
  (resolved at spawn and printed), --parent to you then to the one registered
  coordinator; the harness is the preset's.
  Overrides: --lane <id>, --tmux <name>, --base-sha <sha>.
  Model preset: --preset flash4 resolves through the platform config directory's
  boop/config.json; `boop config presets` lists every name with its model, bin
  and harness.
  Alternate binary: --bin ccz runs the harness as that executable instead of its
  own (ccz is claude under the z.ai env); a preset's `bin` key sets it per name.
  Completion assertions: --expect-path <rel> (repeatable) names a worktree file
  that must exist; --expect-commit-subject <text> (repeatable) an exact commit
  subject after base-sha; --expect-commits-at-least <n> a floor on those commits.
  One shot: worktree at base sha + spawn + route registration.
  Always --dry-run first; the printed `cmd:` line is the literal spawn.

COMPLETION: the supervisor writes ONE row `lane <id> done rc=<n>` into the
  parent's mailbox on every exit path, including a signalled pane; the pane's
  own epilogue only drops the lane's route.
  A lane spawned with --parent reports completion; do not poll.
  A parent whose route is kind=coordinator (what `boop tui <harness>` writes)
  gets that hail through its harness door as its next prompt; no wait needs
  arming.
  `--wait` blocks on that row and exits with the lane's rc, so spawn-and-join is
  one command; `--wait-timeout <s>` (default 3600, 0 waits forever) exits 124.
  The same wait after the fact is the one wait verb, given the lane's name:
    boop wait <lane> [--wait-timeout <s>]
  A wait whose lane route goes dead with no row exits 3 instead of blocking.

RETIRE + REVIVE: a lane whose result row is written and then sees no mail for
  BOOP_IDLE_SHUTDOWN_SECS (default 60; 0 disables) closes its harness and
  exits with the rc it already mailed; residency reads `retired` and the
  parent gets one `note` row. Nothing is lost: the conversation id is pinned
  in ~/.agent/lanes/<lane>/conversation and the exact spawn in spawn.json.
    boop beep <lane> <body>
  to a retired lane replays that spawn, re-registers the route, resumes the
  pinned conversation, waits up to 60 s for the supervisor to report live,
  and hands it the body as its opening turn. The send's wait then ends on the
  lane's next yield or result row, the same rows its parent reads.

DEBUG: what just went wrong, without opening a log:
    boop debug [--since 2m] [--lane <id>] [--json]
  The WARN/ERROR tail of every ~/.agent/lanes/<lane>/supervise.log plus the
  store's kind=error trace events, grouped by lane, oldest first inside a lane.
  Named, it answers one lane in full, five sections, `none` for an empty one:
    boop debug <lane>
  1 route (kind, harness, model, session, cwd, parent, liveness, last turn),
  2 the last 5 mail rows with the rung each landed on, 3 the worktree's last 5
  commits and its dirty count, 4 the last 3 assistant turns and tool calls,
  5 the alert window above.
  `boop --help` prints a one-line banner when that window is non-empty, and
  nothing when it is clean.

LIVENESS: a lane can die silently, producing nothing. Liveness is TWO checks:
    1. process alive:    boop beep ps <lane>
    2. worktree changed: git -C <worktree> status --short
  A REPORT.md at the root alone proves nothing; check its mtime and first line
  against the lane you dispatched.
  `boop beep lane list --all` adds what the registry does not hold: unregistered
  tmux sessions and claude Agent-tool worktrees, with measured liveness for
  pane-less routes.

TRANSPORT: every lane pane runs ONE command, whatever the harness:
    boop beep lane run --lane <id> --harness <h> --brief <abs> --model <m>
  That supervisor owns the harness conversation and the lane's mailbox. It opens
  the conversation with the brief, drains the mailbox every 700 ms, and starts a
  resume turn for anything the harness would not take mid-turn. Nothing is ever
  dropped and no hail needs a human re-dispatch.

DELIVERY: what one send does after the row is written.
  A kind=lane route is handed to its supervisor (stream-json stdin for claude,
  app-server turn/steer for codex). A kind=coordinator route (a pane running
  `boop tui <harness>`) goes through the harness door; nothing is typed:
    claude    unix socket `~/.claude/sessions/<pid>.json` names; next turn boundary
    codex     `codex queue --thread <id> --remote` on the remote-control daemon
    opencode  `POST /session/<id>/prompt_async` on boop's `opencode serve` (:4097)
    kimi      no door; spawn a lane instead
  The recipient takes it as its next prompt; no agent reads a mailbox. A route
  with no harness, or a session the door cannot find, walks down the ladder to
  the hook inbox, the pane, then the mailbox; no send reports a refusal.
  Proof of delivery is the transition history, one row per rung the ladder
  walked (appended, held-for-turn-boundary, queued-in-hook-inbox,
  pasted-into-pane, held-in-mailbox, accepted-by-harness):
    boop db \"SELECT * FROM agent_delivery_transition ORDER BY sequence\"
  and `boop wait <message-id>` prints that history.

SEND: one verb, `boop beep`. It sends and then blocks for the answer:
    boop beep <route> <body> [--timeout <s>] [--kind <k>] [--as <name>]
    boop beep <route> <body> --no-wait          send and return
  <route> is a lane, a coordinator, a native, or one of two aliases:
    boop beep parent \"done with x\"      the caller's own parent edge
    boop beep children \"stop\"           every live child of the caller
  Neither end of an alias edge is spelled by the caller; the registry holds it.
  It walks the same ladder every send walks, prints the rung that took the row,
  then blocks. Exits: 0 on a reply or the recipient's turn ending, 124 on the
  timeout, 3 when the route dies first. The last line is always the next
  command: `boop wait <id>` after an answer, `boop debug <route>` after a
  failure. `--as <name>` is the sender when the whoami ladder cannot say it,
  the same spelling `boop wait --me --as <name>` takes.
  A route named after a `beep` subcommand (lane, agent, ps, pstree, harness) is
  unreachable and says so; rename it.

WAIT: every agent can background a shell, so the universal push is a block.
  A wait on a door-delivered hail also ends when the recipient's turn ends
  (claude registry status, codex thread/status/changed, opencode session.idle),
  printing `<route> turn ended (<status>)`; a reply mail ends it sooner.
    boop wait <message-id>          the reply to what you just sent
    boop wait <lane>                a registered lane's result row, its rc
    boop wait --me [--as <name>]    the next unread mail addressed to you
  Default timeout 540s (under the 10-minute cap a background shell gives you),
  `--wait-timeout <s>` overrides it, and a timeout exits 124 printing the
  re-run line on stdout AND stderr. A lane whose typed expectations are unmet
  after a clean exit is rewritten to exit 4, the \"task incomplete\" exit, with
  the failed assertions in the row's detail; a lane route that goes dead with
  no result row exits 3. A reply is a row naming your id in `reply_to`, or the
  recipient's next mail back to you. Every arrival is
  printed and stamped delivered, so a second wait on the same id blocks
  instead of replaying it. The LAST line of every exit is the next command to
  run; nobody composes one by hand.

ACK: age-based bulk-mark, NOT proof-of-read:
    boop beep message ack
  An ack proves a read at best, never compliance; compliance is the work's own
  artifacts.

ROUTE: the session id one lane answers on, whose route cwd is its worktree:
    boop beep lane route <lane>
  Mailbox: ~/.agent/boop.db (agent_mail + agent_route); --mail-dir names the
  directory holding it.

TRACE + PURPOSE: a session id is per-process-run and MOVES on /clear, on
compaction and on resume. A trace does not move, and every session id a lane
ever wears hangs under one:
    agent_trace       trace_id, root_session_id, started_ts
    agent_trace_span  session_id -> trace_id, attach_id (WHY it attached)
    agent_lane        one row per spawn: goal text, brief path id, brief body id
    markdown_cache    digest UNIQUE, body, bytes, first_ts (briefs dedupe here)
  `lane create` opens `trace-<lane>`; `--trace <id>` continues an existing one.
  A session attaches only on evidence boop holds: lane-create, lane-run,
  supervisor-conversation, backfill-spawned-edge. Adjacency in time is NOT
  evidence, so an unattached session stays unattached; a wrong attach would
  silently merge two arcs.
    boop db \"SELECT t.value, d.value FROM agent_trace_span s
      JOIN dict_trace t ON t.id=s.trace_id
      JOIN dict_session d ON d.id=s.session_id\"
  The brief body is stored AS OF SPAWN. Editing the file afterward does not
  change what the store says the lane was told.

STORE SCHEMA: this build writes version {version}. A store written by an older
build is refused, and `boop db sync create --rebuild` drops every stored row
and re-projects every transcript from byte 0 (about 18 s over 1.5 GB here).
Nothing is wiped without that flag.

SQL: the store is SQLite at ~/.agent/boop.db; `boop db \"<sql>\"` queries it
  read-only. sqlite3 dot-commands (.schema, .tables) are NOT supported; the
  passthrough takes plain SQL only.

BOOP_NO_SYNC=1 in the environment skips the startup transcript sync for every
  verb, so a read hits the store as it stands instead of paying a cold sync.

READ: the questions agents ask most, each one verb, no SQL and no schema probe:
    boop db search <text> [--days 7] [--harness H] [--limit 50]   who said X
    boop db sessions [--days 7] [--harness H]   what ran where: id, harness,
                                                cwd, branch, turns, last_ts
    boop db lanes [--days 7]                    spawns with model, branch,
                                                parent, goal, result rc+detail
    boop db mail <route> [--kind result]        one route's inbox and outbox
    boop db schema                              every table with its columns
    boop db status [--window <min>]             who is alive and what it cost
    boop db usage burn-rate                     tokens/min, dollars/hour
    boop db price list                          the model price table
  `--format text` prints tab-separated rows; the default is NDJSON. Every
  text column in the store is an id into a `dict_*` table (`agent_turn` holds
  `role_id`, `session_id`, and its text in `said`); the verbs above do those
  joins so a hand-written query is the exception.

FAVORITE: pin markdown you want to keep, read it back later:
    boop me favorite -1 --note <why>      the newest assistant turn of the
      caller's own conversation; -2 is the one before, and -1 is the default
    boop db favorite add --file <md> [--note <why>] [--source <text>]
    boop db favorite list --limit 10 --format text
    boop db favorite show <id>
    boop db favorite edit <id> --note <why>
    boop db favorite delete <id>
  `me` resolves the caller from BOOP_SESSION, so run it inside the pane whose
  turn you want. Bodies dedupe through markdown_cache and are immutable; note
  and source are editable.

ME: the caller's own conversation.
    boop me mood [--as <name>]        the mood template hails render with
    boop me favorite -1               see FAVORITE

SHELL: `eval \"$(boop shell-init bash)\"` defines codex, claude, ccz, kimi and
  opencode as functions. Inside tmux they run `boop tui <harness>`, registering
  the pane. Outside tmux they register a pane-less coordinator <entry>-<dir>
  and stamp BOOP_SESSION, so fresh launches and resumes keep one boop id per
  directory. Every wrapped TUI logs to
  ~/.agent/lanes/<harness>-<pane>/supervise.log and never into its own screen.

IDENTITY: two rungs only: `--as <name>`, then the BOOP_SESSION env stamp.
  `boop tui` writes the stamp; a session that predates it passes
  BOOP_SESSION=<name> on spawns or `--as` on every verb. A native subagent
  shares its spawner's process, so the stamp names the spawner:
    boop beep agent register <name> --parent <route>
  prints the instruction; every verb the native runs carries `--as <name>`.
  A bare `wait --me` under a lane stamp with live native children is refused
  with the candidates listed. `boop whoami` prints which rung named you.

PRESETS: model spelling is presets only; `boop config presets` lists name,
  harness, model, effort, bin. Lane defaults: flash4 or pro4; luna for codex
  (sol only on an explicit ask); k3 for kimi; glm53 for claude through z.ai
  (bin ccz). The codex/gpt and claude families through opencode are refused at
  spawn: each has a flat-rate harness and opencode bills them metered. Gemini is allowed.

LAWS:
  1 Every lane spawn goes through `lane create`; a bare tmux spawn leaves no
    edge and no tracking.
  2 Claude-model workers on the user's own plan are the coordinator's native
    subagents (Agent tool). Lanes are for opencode, codex, kimi and ccz.
  3 A lane can die silently. Liveness is TWO checks: `boop beep ps <lane>`
    AND `git -C <worktree> status --short`. A REPORT.md alone proves nothing.
  4 Give each lane its own CARGO_TARGET_DIR; shared target dirs race.
  5 A brief never writes an absolute `cd` to the primary checkout; the lane
    works in $PWD, its worktree.
  6 `lane delete --state dead` removes each dead lane's own worktree and
    nothing above it; `--dry-run` first. Nothing in boop runs rm -rf on
    .boop-worktrees.
  7 `boop beep message ack` proves a read at best, never compliance.
  8 Codex native subagents need sandbox_mode=danger-full-access plus ACP
    session mode agent-full-access, or their boop calls cannot write the mail
    dir or .git/worktrees.

BUILD: hafley-rs crates/boop; `cargo install --path crates/boop --force` from
  main installs ~/.cargo/bin/boop. `boop --version` prints version and sha.",
        version = ident::SCHEMA_VERSION
    )
}

/// Write one line, treating a closed pipe as a normal end. Rust masks SIGPIPE,
/// so a bare `println!` panics the moment output is piped into `head`.
pub(crate) fn line(text: &str) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    match write_line(&mut out, text) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => std::process::exit(0),
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "write failed: {error}");
            std::process::exit(1);
        }
    }
}

pub(crate) fn write_line(output: &mut impl std::io::Write, text: &str) -> std::io::Result<()> {
    writeln!(output, "{text}")
}

pub(crate) fn mail_dir(value: Option<&Path>) -> Result<PathBuf> {
    match value {
        Some(path) => Ok(path.to_path_buf()),
        None => bus::default_mail_dir(),
    }
}

/// Pad `value` to `width` with trailing spaces (Rust strings pad a mix of
/// byte and char semantics; bus uses JS padEnd which pads code units, close
/// enough for lane names here).
pub(crate) fn pad(value: &str, width: usize) -> String {
    let mut out = value.to_owned();
    if out.chars().count() < width {
        out.extend(std::iter::repeat_n(' ', width - out.chars().count()));
    }
    out
}

pub(crate) fn write_route(dir: &std::path::Path, lane_id: &str, route: Route) -> Result<()> {
    let path = dir.join("registry.json");
    bus::cas_update_json(&path, |current| {
        current.insert(lane_id.to_owned(), route_to_json(&route));
        Ok(())
    })
}

pub(crate) fn route_to_json(route: &Route) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    object.insert("kind".into(), serde_json::json!(route.kind));
    if let Some(harness) = &route.harness {
        object.insert("harness".into(), serde_json::json!(harness));
    }
    if let Some(tmux) = &route.tmux {
        object.insert("tmux".into(), serde_json::json!(tmux));
    }
    if let Some(cwd) = &route.cwd {
        object.insert("cwd".into(), serde_json::json!(cwd));
    }
    if let Some(model) = &route.model {
        object.insert("model".into(), serde_json::json!(model));
    }
    if let Some(mode) = &route.mode {
        object.insert("mode".into(), serde_json::json!(mode));
    }
    if let Some(session_id) = &route.session_id {
        object.insert("sessionId".into(), serde_json::json!(session_id));
    }
    if let Some(source_path) = &route.source_path {
        object.insert("sourcePath".into(), serde_json::json!(source_path));
    }
    if let Some(parent) = &route.parent {
        object.insert("parent".into(), serde_json::json!(parent));
    }
    if let Some(goal) = &route.goal {
        object.insert("goal".into(), serde_json::json!(goal));
    }
    if let Some(registered_at) = &route.registered_at {
        object.insert("registeredAt".into(), serde_json::json!(registered_at));
    }
    if let Some(base_sha) = &route.base_sha {
        object.insert("baseSha".into(), serde_json::json!(base_sha));
    }
    if let Some(worktree_dir) = &route.worktree_dir {
        object.insert("worktreeDir".into(), serde_json::json!(worktree_dir));
    }
    if let Some(socket) = &route.app_server_socket {
        object.insert("appServerSocket".into(), serde_json::json!(socket));
    }
    serde_json::Value::Object(object)
}

pub(crate) fn append_message(dir: &std::path::Path, message: &bus::Message) -> Result<()> {
    append_message_to(dir, "bus", message)
}

/// `filename` is the old ndjson spelling of a mailbox name; its stem is the
/// `mailbox` column now.
pub(crate) fn append_message_to(
    dir: &std::path::Path,
    filename: &str,
    message: &bus::Message,
) -> Result<()> {
    let mailbox = filename.strip_suffix(".ndjson").unwrap_or(filename);
    bus::append(dir, mailbox, message).context("append mailbox row")
}

pub(crate) fn append_ack(
    dir: &std::path::Path,
    _box_name: Option<&str>,
    message: &bus::Message,
) -> Result<()> {
    append_acks(dir, std::slice::from_ref(message)).map(|_| ())
}

/// Take delivery of a whole batch in one open: a drained inbox is N rows and a
/// per-row append would be N opens of the same file.
pub(crate) fn append_acks(dir: &std::path::Path, messages: &[bus::Message]) -> Result<usize> {
    if messages.is_empty() {
        return Ok(0);
    }
    let ids: Vec<String> = messages.iter().map(|message| message.id.clone()).collect();
    let store = bus::open_store(dir).context("open the mailbox")?;
    bus::ack_messages(&store, &ids, &bus::now_iso()).context("stamp mailbox rows taken")?;
    Ok(messages.len())
}

#[cfg(feature = "agent-read")]
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
pub(crate) mod testkit {
    use std::path::PathBuf;

    use boop::bus::Route;
    use boop::harness::HarnessId;

    pub(crate) fn temp_mail_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        std::env::temp_dir().join(format!(
            "boop_mail_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    pub(crate) fn route_with(parent: Option<&str>) -> Route {
        Route {
            kind: "lane".into(),
            harness: Some(HarnessId::Opencode),
            tmux: Some("lane-x".into()),
            cwd: None,
            model: None,
            mode: None,
            session_id: None,
            source_path: None,
            parent: parent.map(str::to_owned),
            goal: None,
            registered_at: None,
            base_sha: None,
            worktree_dir: None,
            app_server_socket: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::testkit::temp_mail_dir;
    use boop::bus::{read_routes, Route};

    /// RECEIPT (job 1). A route written with --goal round-trips through the
    /// registry.
    #[test]
    fn route_goal_round_trips() {
        use boop::harness::HarnessId;
        let dir = temp_mail_dir();
        let route = Route {
            kind: "lane".into(),
            harness: Some(HarnessId::Opencode),
            tmux: Some("lane-x".into()),
            cwd: None,
            model: None,
            mode: None,
            session_id: None,
            source_path: None,
            parent: None,
            goal: Some("ship the edge".into()),
            registered_at: None,
            base_sha: None,
            worktree_dir: None,
            app_server_socket: None,
        };
        write_route(&dir, "child", route).unwrap();
        let routes = read_routes(&dir).unwrap();
        assert_eq!(
            routes["child"].goal.as_deref(),
            Some("ship the edge"),
            "registry: {:#?}",
            routes
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
