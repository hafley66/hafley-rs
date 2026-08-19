pub(crate) mod db;
pub(crate) mod job;
pub(crate) mod mail;
pub(crate) mod me;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use boop::bus::Route;
use boop::event::AgentEvent;
use boop::{bus, ident};

use crate::OutputFormat;

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

WARMUP: after the worktree exists and before the agent starts, lane create runs
  the repo's `boop-start` just recipe if it declares one, and a repo that does
  not is skipped in silence. A FAILING recipe blocks the spawn: the pre-commit
  hook needs what it installs, and a lane that cannot commit reads the abort as
  success. `--no-start` opts out.

SPAWN: every lane spawn goes through lane create; bare tmux spawns leave no
edge and stay invisible to tracking:
    boop beep lane create --branch feature/<name> --brief <abs-path> \\
      [--goal <text>] [--model <m>] [--wait] [--mail-dir <d>] [--dry-run]
  ONE derivation, from the whole branch name: `feature/schema-emit` gives lane
  id and tmux session `feature-schema-emit` (`/` spelled `-`, the one character
  tmux cannot hold) and worktree `.boop-worktrees/feature/schema-emit` (the same
  name as a path). No prefix is dropped and no `lane/` prefix is added.
  Kinds are feature/fix/refactor/chore, a convention the CLI prints, not a gate.
  --cwd defaults to the repo you stand in, --base-sha to origin/main's head
  (resolved at spawn and printed), --parent to you then to the one registered
  coordinator, --harness to the one the model spelling names (gpt-* codex,
  provider/model opencode, kimi-* kimi).
  Overrides: --lane <id>, --tmux <name>, --base-sha <sha>, --harness <id>.
  Model preset: --preset flash4 resolves through the platform config directory's
  boop/config.json; `boop config presets` lists every name with its model and harness.
  One shot: worktree at base sha + spawn + route registration.
  Always --dry-run first; the printed `cmd:` line is the literal spawn.

COMPLETION: the supervisor writes ONE row `lane <id> done rc=<n>` into the
  parent's mailbox on every exit path, including a signalled pane; the pane's
  own epilogue only drops the lane's route.
  A lane spawned with --parent reports completion; do not poll.
  A parent whose route is kind=coordinator (what `boop adopt` writes) gets that
  hail TYPED INTO ITS PANE, mid-turn or idle; no wait needs arming.
  `--wait` blocks on that row and exits with the lane's rc, so spawn-and-join is
  one command; `--wait-timeout <s>` (default 3600, 0 waits forever) exits 124.
  The same wait after the fact is `boop beep lane wait <lane>`.
  A wait whose lane route goes dead with no row exits 3 instead of blocking.

DEBUG: what just went wrong, without opening a log:
    boop debug [--since 2m] [--lane <id>] [--json]
  The WARN/ERROR tail of every ~/.agent/lanes/<lane>/supervise.log plus the
  store's kind=error trace events, grouped by lane, oldest first inside a lane.
  `boop --help` prints a one-line banner when that window is non-empty, and
  nothing when it is clean.

LIVENESS: a lane can die silently, producing nothing. Liveness is TWO checks:
    1. process alive:    boop beep ps <lane>
    2. worktree changed: git -C <worktree> status --short
  A REPORT.md at the root alone proves nothing; check its mtime and first line
  against the lane you dispatched.

TRANSPORT: every lane pane runs ONE command, whatever the harness:
    boop beep lane run --lane <id> --harness <h> --brief <abs> --model <m>
  That supervisor owns the harness conversation and the lane's inbox. It opens
  the conversation with the brief, drains the inbox every 700 ms, and starts a
  resume turn for anything the harness would not take mid-turn. Nothing is ever
  dropped and no hail needs a human re-dispatch.

HAIL: boop beep hail <lane> --body \"text\" [--from <me>] [--kind <k>]
  Reaches a running lane MID-TURN on all four harnesses:
    claude    stream-json user line on the child's stdin
    codex     app-server turn/steer against the live turn id
    opencode  typed into its TUI window; plain Enter is the steer
    kimi      typed into its TUI window, then C-s (Enter alone only QUEUES)
  A harness with no in-flight port would report `nextturn` and the supervisor
  would hold the text for a resume turn; none does today.
  A kind=coordinator route (an adopted pane) is delivered by literal keystrokes
  plus Enter into its pane; a kind=lane route is left for its supervisor.
  Proof of delivery is in the store, not in a screenshot:
    boop db \"SELECT * FROM agent_edge\" -- edge kind deliver-midturn/deliver-nextturn
  and the mailbox row's to_timestamp is stamped when the lane takes it.

TELL: boop tell-parent [--kind completion|yield|note] [--body \"t\"] mails the
  caller's parent edge; boop tell-children --body \"t\" mails every live child.

WAIT: every agent can background a shell, so the universal push is a block:
    boop wait <message-id>          the reply to what you just sent
    boop wait --me [--as <name>]    the next unread mail addressed to you
    boop beep hail <lane> --body \"...\" --wait-timeout <s>   send, then block
  Default timeout 540s (under the 10-minute cap a background shell gives you),
  `--wait-timeout <s>` overrides it, and a timeout exits 124 printing the
  re-run line on stdout AND stderr. A reply is a row naming your id in
  `reply_to`, or the recipient's next mail back to you. Every arrival is
  printed and stamped delivered, so a second wait on the same id blocks
  instead of replaying it. The LAST line of every exit is the next command to
  run; nobody composes one by hand.

ACK: boop beep message ack is age-based bulk-mark, NOT proof-of-read. An ack
  proves read at best, never compliance; compliance = the work's own artifacts.

ROUTE: session id for a lane: boop beep lane route <lane> (route cwd = the
  lane's worktree). Mailbox: ~/.agent/mail/ (bus.ndjson + registry.json),
  override with --mail-dir.

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

The pre-split verbs (harnesses, sessions, events, chat, tail, list, measure,
dispatch, lane, resolve, adopt, sweep, prune, hail, sync, follow) still run as
hidden aliases for one release. Use `beep` and `db`.",
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

// ---------------------------------------------------------------------------
// The verb output helpers
// ---------------------------------------------------------------------------

pub(crate) fn emit_event(event: &AgentEvent, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            if let Ok(encoded) = serde_json::to_string(event) {
                println!("{encoded}");
            }
        }
        OutputFormat::Text => {
            let paths = event
                .paths
                .iter()
                .map(|path| format!("{}({:?})", path.path, path.access))
                .collect::<Vec<_>>()
                .join(",");
            let tool = event.tool_name.as_deref().unwrap_or("-");
            if paths.is_empty() && event.urls.is_empty() {
                println!(
                    "[{}] {} {} {} {}",
                    event.harness, event.ts_ms, event.record_type, tool, event.session_id
                );
            } else {
                println!(
                    "[{}] {} {} {} {} paths=[{}] urls=[{}]",
                    event.harness,
                    event.ts_ms,
                    event.record_type,
                    tool,
                    event.session_id,
                    paths,
                    event.urls.join(",")
                );
            }
        }
    }
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
    serde_json::Value::Object(object)
}

pub(crate) fn append_message(dir: &std::path::Path, message: &bus::Message) -> Result<()> {
    append_message_to(dir, "bus.ndjson", message)
}

pub(crate) fn append_message_to(
    dir: &std::path::Path,
    filename: &str,
    message: &bus::Message,
) -> Result<()> {
    let path = dir.join(filename);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("create mail dir")?;
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .context("open ndjson box")?;
    writeln!(file, "{}", bus::message_line(message)).context("append ndjson box")?;
    Ok(())
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
    let path = dir.join("bus.ndjson");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("create mail dir")?;
    }
    let stamp = bus::now_iso();
    let mut batch = String::new();
    for message in messages {
        let mut ack = message.clone();
        ack.to_timestamp = Some(stamp.clone());
        batch.push_str(&bus::message_line(&ack));
        batch.push('\n');
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .context("open ndjson box")?;
    file.write_all(batch.as_bytes())
        .context("append ndjson box")?;
    Ok(messages.len())
}

#[cfg(feature = "agent-read")]
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
