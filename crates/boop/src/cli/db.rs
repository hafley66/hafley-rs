use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tracing::info;

use boop::registry::Registry;
use boop::{bus, ident, tmux};
#[cfg(feature = "agent-read")]
use boop::{query, usage};

use crate::cli::job::lane_state;
use crate::cli::{emit_event, line, mail_dir, now_ms};
use crate::{
    AgentSessionGraphFormat, AgentSummaryCmd, AgentSummaryFormat, ChatCmd, CursorCmd, DbCmd,
    EdgeCmd, FactCmd, FavoriteCmd, OutputFormat, PriceCmd, QueryArgs, QueryFormat, SessionCmd,
    SyncCmd, TurnCmd, UsageArgs, UsageCmd,
};

// ---------------------------------------------------------------------------
// Pass 1 verbs: layer 2 (transcript)
// ---------------------------------------------------------------------------

pub(crate) fn run_harnesses(registry: &Registry) -> Result<()> {
    for harness in registry.all() {
        line(harness.id());
    }
    Ok(())
}

pub(crate) fn run_sessions(registry: &Registry, harness_id: Option<&str>) -> Result<()> {
    let harnesses: Vec<&dyn boop::harness::Harness> = match harness_id {
        Some(id) => vec![resolve_harness(registry, id)?],
        None => registry.all().iter().map(|boxed| boxed.as_ref()).collect(),
    };
    for adapter in harnesses {
        for session in adapter.sessions()? {
            line(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                session.session_id,
                session.harness,
                session.cwd.as_deref().unwrap_or("-"),
                session.git_branch.as_deref().unwrap_or("-"),
                session.modified_ms,
                session.size,
            ));
        }
    }
    Ok(())
}

pub(crate) fn run_tail(
    registry: &Registry,
    session_id: &str,
    offset: u64,
    format: OutputFormat,
) -> Result<()> {
    for adapter in registry.all() {
        for session in adapter.sessions()? {
            if session.session_id == session_id {
                let chunk = adapter.read_from(&session, offset)?;
                emit_notes(chunk.reset, chunk.skipped);
                for event in &chunk.events {
                    emit_event(event, format);
                }
                if matches!(format, OutputFormat::Text) {
                    eprintln!("resume offset: {}", chunk.next_offset);
                }
                return Ok(());
            }
        }
    }
    anyhow::bail!("no session found with id `{session_id}`")
}

/// Resolve the shared filter set, with the session filter pinned externally
/// so `--all` can clear it.
pub(crate) fn query_from(q: &QueryArgs, session: Option<String>) -> ident::TurnQuery {
    ident::TurnQuery {
        harness: q.harness.clone(),
        session,
        role: q.role.clone(),
        since: q.since,
        until: q.until,
        turn_from: q.turn_from,
        turn_to: q.turn_to,
        path: q.path.clone(),
        limit: q.limit,
    }
}

/// Query the db with the shared filter set; emit raw rows, no interpretation.
/// Turns first, then any spawn edges touching the filtered session.
pub(crate) fn run_query(query: &QueryArgs) -> Result<()> {
    let store = ident::Store::open_readonly(ident::Store::default_path()?)?;
    let rows = store.query_turns(&query_from(query, query.session.clone()))?;
    emit_rows(&rows, query.format);
    emit_edges(&store, query.session.as_deref(), query.limit)?;
    Ok(())
}

/// `all` clears the session filter; `follow` re-queries in a loop.
#[derive(Clone, Copy, Default)]
pub(crate) struct ChatQueryOptions {
    pub(crate) all: bool,
    pub(crate) follow: bool,
}

/// `boop chat`: like `query` but emits the chat-repr turn shape. `query.format`
/// already selects NDJSON vs text, so there is no separate JSON flag here.
pub(crate) fn run_chat_query(query: &QueryArgs, opts: ChatQueryOptions) -> Result<()> {
    let store = ident::Store::open_readonly(ident::Store::default_path()?)?;
    let session = if opts.all {
        None
    } else {
        query.session.clone()
    };
    if opts.follow {
        loop {
            let rows = store.query_turns(&query_from(query, session.clone()))?;
            emit_rows(&rows, QueryFormat::Ndjson);
            std::io::Write::flush(&mut std::io::stdout())?;
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    let rows = store.query_turns(&query_from(query, session.clone()))?;
    emit_rows(&rows, query.format);
    Ok(())
}

pub(crate) fn emit_edges(
    store: &ident::Store,
    session: Option<&str>,
    limit: Option<u64>,
) -> Result<()> {
    let edges = store.query_edges(session)?;
    for edge in edges.into_iter().take(limit.unwrap_or(u64::MAX) as usize) {
        line(&serde_json::to_string(&edge)?.to_string());
    }
    Ok(())
}

pub(crate) fn emit_rows(rows: &[ident::Row], format: QueryFormat) {
    for row in rows {
        match format {
            QueryFormat::Ndjson => {
                if let Ok(encoded) = serde_json::to_string(row) {
                    line(&encoded);
                }
            }
            QueryFormat::Text => {
                line(&format!(
                    "{} {} {} {} {}",
                    row["session"].as_str().unwrap_or(""),
                    row["turn"].as_i64().unwrap_or(0),
                    row["role"].as_str().unwrap_or(""),
                    row["ts"].as_i64().unwrap_or(0),
                    row["said"].as_str().unwrap_or(""),
                ));
            }
        }
    }
}

/// `boop sync`: tail every harness forward from stored offsets into the db.
pub(crate) fn run_sync_all(registry: &Registry, rebuild: bool) -> Result<()> {
    sync_all(registry, rebuild, true, SyncLiveness::StampLivePid)
}

/// Whether incremental transcript sync asks tmux for a route PID per changed
/// session. Summary freshness only needs transcript and usage facts; its later
/// runtime projection owns one bounded tmux/process observation.
#[derive(Clone, Copy)]
pub(crate) enum SyncLiveness {
    StampLivePid,
    TranscriptOnly,
}

/// Incrementally synchronize transcript facts. `report` controls only the
/// human progress receipt; callers that must emit one stable document keep it
/// false and write their own result after this returns.
pub(crate) fn sync_all(
    registry: &Registry,
    rebuild: bool,
    report: bool,
    liveness: SyncLiveness,
) -> Result<()> {
    let started = std::time::Instant::now();
    if report {
        info!(rebuild, "transcript sync started");
    }
    let store = ident::Store::open(ident::Store::default_path()?)?;
    if rebuild {
        store.rebuild()?;
    } else {
        refuse_stale(&store)?;
    }
    let known = store.known_sessions()?;
    let mut pending = Vec::new();
    let mut roots_to_stamp = Vec::new();
    for adapter in registry.all() {
        let roots = adapter.session_roots()?;
        let root_stamps_match = !roots.is_empty()
            && roots.iter().all(|root| {
                let mtime_ms = path_modified_ms(root);
                store
                    .root_stamp_matches(adapter.id(), root, mtime_ms)
                    .unwrap_or(false)
            });
        if root_stamps_match && (!adapter.known_paths_can_move() || !known.has_moved(adapter.id()))
        {
            continue;
        }
        let candidates = adapter.sync_candidates(&known)?;
        let had_candidates = !candidates.is_empty();
        for session in candidates {
            store.backfill_cursor_modified(
                &session.session_id,
                &session.path.display().to_string(),
                session.modified_ms,
            )?;
            if session_needs_sync(&session, &known) {
                pending.push((adapter.as_ref(), session));
            }
        }
        if had_candidates {
            roots_to_stamp.push((adapter.id(), roots));
        }
    }
    let routes = match liveness {
        SyncLiveness::StampLivePid => Some(bus::read_routes(&mail_dir(None)?).unwrap_or_default()),
        SyncLiveness::TranscriptOnly => None,
    };
    let mut stat = ident::SyncStat::default();
    for (adapter, session) in pending {
        tracing::debug!(
            harness = adapter.id(),
            session_id = session.session_id,
            "transcript session sync started"
        );
        let pid = sync_session_pid(liveness, || {
            routes
                .as_ref()
                .and_then(|routes| session_route_pid(routes, &session))
        });
        store.begin()?;
        let result = (|| {
            store.project_discovered_session(&session)?;
            ident::sync_session_with_pid(&store, adapter, &session, pid)
        })();
        match result {
            Ok(session_stat) => {
                store.commit()?;
                stat.add(session_stat);
            }
            Err(error) => {
                let _ = store.rollback();
                return Err(error);
            }
        }
    }
    for (harness, roots) in roots_to_stamp {
        for root in roots {
            store.stamp_root(harness, &root, path_modified_ms(&root))?;
        }
    }
    let elapsed_ms = started.elapsed().as_millis();
    if report {
        let counts = store.counts()?;
        let db_bytes = store.db_bytes()?;
        let sparse = store.sparse_sessions()?.len();
        let rate = (stat.written as u128)
            .saturating_mul(1000)
            .checked_div(elapsed_ms.max(1))
            .unwrap_or(0) as u64;
        println!(
            "events={} dropped={} usage_new={} usage_updated={} sparse_sessions={sparse} elapsed_ms={elapsed_ms} rate={rate}/s db_bytes={db_bytes} counts={}",
            stat.written,
            stat.dropped,
            stat.usage_written,
            stat.usage_updated,
            serde_json::to_string(&counts)?
        );
        info!(
            events = stat.written,
            dropped = stat.dropped,
            usage_new = stat.usage_written,
            usage_updated = stat.usage_updated,
            elapsed_ms,
            rate,
            "transcript sync finished"
        );
    }
    Ok(())
}

pub(crate) fn path_modified_ms(path: &std::path::Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

pub(crate) fn session_needs_sync(
    session: &boop::harness::SessionRef,
    known: &boop::harness::KnownSessions,
) -> bool {
    known
        .get_session(&session.path, &session.session_id)
        .is_none_or(|known| known.cursor != session.size)
}

pub(crate) fn sync_session_pid(
    liveness: SyncLiveness,
    acquire: impl FnOnce() -> Option<i64>,
) -> Option<i64> {
    match liveness {
        SyncLiveness::StampLivePid => acquire(),
        SyncLiveness::TranscriptOnly => None,
    }
}

/// The public summary command first refreshes transcript facts without
/// per-session liveness probes, then performs its one bounded runtime summary.
/// A store written before dense ordinals is readable but not appendable, so
/// the refusal names the one command that fixes it.
pub(crate) fn refuse_stale(store: &ident::Store) -> Result<()> {
    if !store.is_stale()? {
        return Ok(());
    }
    anyhow::bail!(
        "store is schema version {}, this boop writes version {}: rows stored under \
         an older schema cannot be appended to. Run `boop db sync create --rebuild` \
         to drop every stored row and re-project every transcript from byte 0.",
        store.schema_version()?,
        ident::SCHEMA_VERSION
    )
}

/// `boop follow`: the same projection on a coarse poll. Sessions and their
/// mtimes are discovered once, and a file is only re-read when its mtime
/// changed, so steady-state idle is a stat per file plus a sleep.
pub(crate) fn run_follow(registry: &Registry) -> Result<()> {
    loop {
        sync_all(registry, false, false, SyncLiveness::StampLivePid)?;
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

/// The pane pid for a session that maps to a lane route (by session id or cwd).
/// A session with no route owns no process and yields `None`, never a guess.
pub(crate) fn session_route_pid(
    routes: &BTreeMap<String, bus::Route>,
    session: &boop::harness::SessionRef,
) -> Option<i64> {
    let route = routes
        .iter()
        .find(|(_, route)| session_matches_route(route, session));
    route
        .and_then(|(_, route)| route.tmux.as_deref())
        .and_then(|target| tmux::mux().pane_pid(None, target))
        .map(i64::from)
}

/// A routeless session must never borrow a pid: two `None` cwds are not a
/// match, only a shared session id or a shared concrete cwd is.
pub(crate) fn session_matches_route(
    route: &bus::Route,
    session: &boop::harness::SessionRef,
) -> bool {
    route.session_id.as_deref() == Some(session.session_id.as_str())
        || (route.cwd.is_some() && route.cwd.as_deref() == session.cwd.as_deref())
}

pub(crate) fn resolve_harness<'a>(
    registry: &'a Registry,
    id: &str,
) -> Result<&'a dyn boop::harness::Harness> {
    registry
        .by_id(id)
        .with_context(|| format!("no harness registered with id `{id}`"))
}

pub(crate) fn emit_notes(reset: bool, skipped: usize) {
    if reset {
        eprintln!("note: transcript shorter than stored offset; restarted from byte 0");
    }
    if skipped > 0 {
        eprintln!("note: skipped {skipped} line(s) that failed to parse as JSON");
    }
}

// ---------------------------------------------------------------------------
// db
// ---------------------------------------------------------------------------

pub(crate) fn run_db(registry: &Registry, cmd: DbCmd) -> Result<()> {
    match cmd {
        #[cfg(feature = "agent-read")]
        DbCmd::AgentSummary { format, mail_dir } => run_agent_summary(format, mail_dir.as_deref()),
        #[cfg(feature = "agent-read")]
        DbCmd::Session { cmd } => match cmd {
            SessionCmd::List { limit, format } => {
                let store = open_ro_store()?;
                emit_json_rows(&store.query_sessions(None, limit)?, format);
                Ok(())
            }
            SessionCmd::Get { session, format } => {
                let store = open_ro_store()?;
                emit_json_rows(&store.query_sessions(Some(&session), None)?, format);
                Ok(())
            }
        },
        DbCmd::Turn { cmd } => match cmd {
            TurnCmd::List { query } => run_query(&query),
            TurnCmd::Get {
                session,
                turn,
                format,
            } => {
                let store = open_ro_store()?;
                let filter = ident::TurnQuery {
                    session: Some(session),
                    turn_from: Some(turn),
                    turn_to: Some(turn),
                    ..Default::default()
                };
                emit_json_rows(&store.query_turns(&filter)?, format);
                Ok(())
            }
        },
        DbCmd::Chat { cmd } => match cmd {
            ChatCmd::List { query, all, follow } => {
                run_chat_query(&query, ChatQueryOptions { all, follow })
            }
        },
        #[cfg(feature = "agent-read")]
        DbCmd::Touch { cmd } => run_fact(query::FactKind::Touch, cmd),
        #[cfg(feature = "agent-read")]
        DbCmd::Command { cmd } => run_fact(query::FactKind::Command, cmd),
        #[cfg(feature = "agent-read")]
        DbCmd::Fetch { cmd } => run_fact(query::FactKind::Fetch, cmd),
        #[cfg(feature = "agent-read")]
        DbCmd::Skill { cmd } => run_fact(query::FactKind::Skill, cmd),
        #[cfg(feature = "agent-read")]
        DbCmd::Pr { cmd } => run_fact(query::FactKind::Pr, cmd),
        #[cfg(feature = "agent-read")]
        DbCmd::Span { cmd } => run_fact(query::FactKind::Span, cmd),
        DbCmd::Edge { cmd } => match cmd {
            EdgeCmd::List { session, limit } => {
                let store = open_ro_store()?;
                emit_edges(&store, session.as_deref(), limit)
            }
        },
        #[cfg(feature = "agent-read")]
        DbCmd::Usage {
            args,
            show_sql,
            cmd,
        } => match cmd {
            None => run_usage(&args, show_sql),
            Some(UsageCmd::Blocks {
                window_hours,
                active,
                args,
            }) => run_usage_blocks(&args, window_hours, active),
            Some(UsageCmd::BurnRate {
                window_minutes,
                args,
            }) => run_usage_burn_rate(&args, window_minutes),
        },
        #[cfg(feature = "agent-read")]
        DbCmd::Price { cmd } => run_price(cmd),
        #[cfg(feature = "agent-read")]
        DbCmd::Favorite { cmd } => match cmd {
            FavoriteCmd::Add { file, note, source } => {
                let body = match file {
                    Some(path) => std::fs::read_to_string(&path)
                        .with_context(|| format!("read {}", path.display()))?,
                    None => {
                        let mut buffer = String::new();
                        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buffer)?;
                        buffer
                    }
                };
                let store = open_store()?;
                let id = store.favorite_add(
                    &body,
                    note.as_deref().unwrap_or(""),
                    source.as_deref().unwrap_or(""),
                    now_ms(),
                )?;
                line(&format!("favorite {id}"));
                Ok(())
            }
            FavoriteCmd::List { limit, format } => {
                let store = open_ro_store()?;
                emit_json_rows(&store.query_favorites(limit)?, format);
                Ok(())
            }
        },
        DbCmd::Sync { cmd } => match cmd {
            SyncCmd::Create { rebuild, forever } => {
                if forever {
                    run_follow(registry)
                } else {
                    run_sync_all(registry, rebuild)
                }
            }
        },
        #[cfg(feature = "agent-read")]
        DbCmd::SyncCursor { cmd } => match cmd {
            CursorCmd::List { limit, format } => {
                let store = open_ro_store()?;
                emit_json_rows(&store.query_sync_cursors(limit)?, format);
                Ok(())
            }
        },
        #[cfg(feature = "agent-read")]
        DbCmd::Status { window, format } => run_status(window, format),
    }
}

pub(crate) fn open_store() -> Result<ident::Store> {
    ident::Store::open(ident::Store::default_path()?)
}

/// `boop db "<sql>": run raw SQL read-only against the store. The open is
/// SQLITE_OPEN_READONLY by flag, so a write is refused by SQLite itself.
pub(crate) fn run_passthrough(sql: &str, format: QueryFormat) -> Result<()> {
    run_passthrough_at(ident::Store::default_path()?, sql, format)
}

pub(crate) fn run_passthrough_at(path: PathBuf, sql: &str, format: QueryFormat) -> Result<()> {
    let store = ident::Store::open_readonly(path)?;
    let (names, rows) = store.passthrough(sql)?;
    match format {
        QueryFormat::Ndjson => {
            for row in &rows {
                line(&serde_json::to_string(row)?);
            }
        }
        QueryFormat::Text => {
            line(&names.join("\t"));
            for row in &rows {
                let Some(object) = row.as_object() else {
                    continue;
                };
                let cells: Vec<String> = names
                    .iter()
                    .map(|name| match object.get(name) {
                        Some(serde_json::Value::String(text)) => text.clone(),
                        Some(serde_json::Value::Null) | None => "-".to_owned(),
                        Some(other) => other.to_string(),
                    })
                    .collect();
                line(&cells.join("\t"));
            }
        }
    }
    Ok(())
}

#[cfg(feature = "agent-read")]
pub(crate) fn run_fact(kind: query::FactKind, cmd: FactCmd) -> Result<()> {
    let FactCmd::List { args } = cmd;
    let store = open_ro_store()?;
    let filter = query::FactQuery {
        session: args.session.clone(),
        since: args.since,
        until: args.until,
        like: args.like.clone(),
        limit: args.limit,
    };
    emit_json_rows(&store.query_facts(kind, &filter)?, args.format);
    Ok(())
}

/// Liveness is asked of tmux once and joined onto the rows; the store cannot
/// know it and a per-row tmux call would be an N+1.
#[cfg(feature = "agent-read")]
pub(crate) fn run_status(window_minutes: u64, format: QueryFormat) -> Result<()> {
    let store = open_ro_store()?;
    let now = now_ms();
    let mut rows = store.query_status(window_minutes * 60_000, now)?;
    let dir = mail_dir(None)?;
    let routes = bus::read_routes(&dir).unwrap_or_default();
    let live = tmux::mux().live_sessions(None);
    for row in &mut rows {
        let session = row["session"].as_str().unwrap_or("").to_owned();
        let lane = routes.iter().find(|(_, route)| {
            route.session_id.as_deref() == Some(session.as_str())
                || route.cwd.as_deref() == row["cwd"].as_str()
        });
        let (lane_name, state) = match lane {
            Some((name, route)) => (Some(name.clone()), lane_state(&live, route)),
            None => (None, "unknown"),
        };
        if let Some(object) = row.as_object_mut() {
            object.insert("lane".into(), serde_json::json!(lane_name));
            object.insert("state".into(), serde_json::json!(state));
        }
    }
    emit_json_rows(&rows, format);
    Ok(())
}

#[cfg(feature = "agent-read")]
pub(crate) fn run_agent_summary(
    format: AgentSummaryFormat,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    let store = open_ro_store()?;
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let mut messages = Vec::new();
    for path in bus::read_boxes(&dir)? {
        messages.extend(bus::parse_box(&path));
    }
    let summary = boop::agent_summary_now(&store, &routes, &messages)?;
    match format {
        AgentSummaryFormat::Json => line(&serde_json::to_string(&summary)?),
        AgentSummaryFormat::Text => line(&agent_summary_text(&summary)),
    }
    Ok(())
}

#[cfg(feature = "agent-read")]
pub(crate) fn run_public_agent_command(cmd: AgentSummaryCmd) -> Result<()> {
    match cmd {
        AgentSummaryCmd::Summary { format, mail_dir } => {
            run_agent_summary(format, mail_dir.as_deref())
        }
        AgentSummaryCmd::Sessions {
            cwd,
            history,
            tmux,
            history_since_ts,
            format: AgentSessionGraphFormat::Json,
            mail_dir,
        } => run_agent_sessions(cwd, history, tmux, history_since_ts, mail_dir.as_deref()),
    }
}

#[cfg(feature = "agent-read")]
pub(crate) fn run_agent_sessions(
    cwd: Option<PathBuf>,
    include_history: bool,
    tmux: Option<String>,
    history_since_ts: Option<u64>,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    let store = open_ro_store()?;
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let mut messages = Vec::new();
    for path in bus::read_boxes(&dir)? {
        messages.extend(bus::parse_box(&path));
    }
    let processes = boop::proc::SysinfoSnapshot::capture()?;
    let graph = boop::load_agent_session_graph_with_runtime(
        &store,
        agent_session_graph_query(cwd, include_history, tmux, history_since_ts),
        boop::AgentSessionGraphRuntime {
            routes: &routes,
            messages: &messages,
            multiplexer: boop::tmux::mux(),
            tmux_socket: None,
            processes: &processes,
        },
    )?;
    line(&serde_json::to_string(&graph)?);
    Ok(())
}

#[cfg(feature = "agent-read")]
pub(crate) fn agent_session_graph_query(
    cwd: Option<PathBuf>,
    include_history: bool,
    tmux: Option<String>,
    history_since_ts: Option<u64>,
) -> boop::AgentSessionGraphQuery {
    boop::AgentSessionGraphQuery {
        cwd,
        include_history,
        tmux,
        history_since_ts,
    }
}

#[cfg(feature = "agent-read")]
pub(crate) fn agent_summary_text(summary: &boop::AgentSummary) -> String {
    use std::fmt::Write;

    let mut output = format!(
        "schema_version\t{}\nactive_agents\t{}\nlane\ttrace\troot_session\tsession\tparent\troute\tcwd\ttmux_target\ttmux_pane\tpid\treported_status\ttmux_liveness\tprocess_liveness\tcompletion\tinbox\toutbox\tunacknowledged\tworktree_route_cwd\tworktree_process_cwd\tdiagnostics\tuser\tassistant\ttool_call\ttotal\tcalls\tinput_tokens\toutput_tokens\tcache_create_5m_tokens\tcache_create_1h_tokens\tcache_read_tokens",
        summary.schema_version, summary.active_agents
    );
    for agent in &summary.agents {
        let runtime = &agent.runtime;
        output.push('\n');
        write!(
            output,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            runtime.lane,
            runtime.trace.as_deref().unwrap_or("-"),
            runtime.root_session.as_deref().unwrap_or("-"),
            runtime.session.as_deref().unwrap_or("-"),
            runtime.parent.as_deref().unwrap_or("-"),
            json_cell(&runtime.route),
            runtime.cwd.as_deref().unwrap_or("-"),
            runtime.tmux_target.as_deref().unwrap_or("-"),
            runtime.tmux_pane.as_deref().unwrap_or("-"),
            runtime.pid.map(|pid| pid.to_string()).unwrap_or_else(|| "-".into()),
            runtime.reported_status.as_deref().unwrap_or("-"),
            tmux_liveness_text(&runtime.liveness.tmux),
            process_liveness_text(&runtime.liveness.process),
            json_cell(&runtime.completion),
            runtime.mailbox.inbox,
            runtime.mailbox.outbox,
            runtime.mailbox.unacknowledged,
            runtime.worktree.route_cwd.as_deref().unwrap_or("-"),
            runtime.worktree.process_cwd.as_deref().unwrap_or("-"),
            json_cell(&runtime.diagnostics),
            agent.activity.user,
            agent.activity.assistant,
            agent.activity.tool_call,
            agent.activity.total,
            agent.activity.calls,
            agent.activity.input_tokens,
            agent.activity.output_tokens,
            agent.activity.cache_create_5m_tokens,
            agent.activity.cache_create_1h_tokens,
            agent.activity.cache_read_tokens,
        )
        .expect("write to string");
    }
    output
}

#[cfg(feature = "agent-read")]
pub(crate) fn json_cell(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".into())
}

#[cfg(feature = "agent-read")]
pub(crate) fn tmux_liveness_text(liveness: &boop::TmuxLiveness) -> &'static str {
    match liveness {
        boop::TmuxLiveness::Live => "live",
        boop::TmuxLiveness::Dead => "dead",
        boop::TmuxLiveness::Inaccessible => "inaccessible",
        boop::TmuxLiveness::Unmanaged => "unmanaged",
    }
}

#[cfg(feature = "agent-read")]
pub(crate) fn process_liveness_text(liveness: &boop::ProcessLiveness) -> &'static str {
    match liveness {
        boop::ProcessLiveness::Live => "live",
        boop::ProcessLiveness::Dead => "dead",
        boop::ProcessLiveness::Unknown => "unknown",
    }
}

pub(crate) fn emit_json_rows(rows: &[ident::Row], format: QueryFormat) {
    match format {
        QueryFormat::Ndjson => {
            for row in rows {
                if let Ok(encoded) = serde_json::to_string(row) {
                    line(&encoded);
                }
            }
        }
        QueryFormat::Text => {
            for row in rows {
                let Some(object) = row.as_object() else {
                    continue;
                };
                let cells: Vec<String> = object
                    .values()
                    .map(|value| match value {
                        serde_json::Value::String(text) => text.clone(),
                        serde_json::Value::Null => "-".to_owned(),
                        other => other.to_string(),
                    })
                    .collect();
                line(&cells.join("\t"));
            }
        }
    }
}

/// The `db usage` alias's report SQL: totals with cost over the whole store.
/// The passthrough is the engine; `--show-sql` prints this const.
#[cfg(feature = "agent-read")]
pub(crate) const USAGE_TOTALS_SQL: &str = "
SELECT COUNT(*) AS calls,
       COALESCE(SUM(usage.input_tokens), 0) AS input_tokens,
       COALESCE(SUM(usage.output_tokens), 0) AS output_tokens,
       COALESCE(SUM(usage.cache_create_5m_tokens), 0) AS cache_create_5m_tokens,
       COALESCE(SUM(usage.cache_create_1h_tokens), 0) AS cache_create_1h_tokens,
       COALESCE(SUM(usage.cache_read_tokens), 0) AS cache_read_tokens,
       SUM(usage.input_tokens / 1e6 * price.input_per_mtok
         + usage.output_tokens / 1e6 * price.output_per_mtok
         + usage.cache_create_5m_tokens / 1e6 * price.cache_write_5m_per_mtok
         + usage.cache_create_1h_tokens / 1e6 * price.cache_write_1h_per_mtok
         + usage.cache_read_tokens / 1e6 * price.cache_read_per_mtok) AS cost_usd
FROM agent_usage AS usage
LEFT JOIN model_price AS price ON price.model_id = usage.model_id";

pub(crate) fn open_ro_store() -> Result<ident::Store> {
    ident::Store::open_readonly(ident::Store::default_path()?)
}

/// `db usage`: the totals report, a thin alias over USAGE_TOTALS_SQL. `--show-sql`
/// prints that const and exits; otherwise it runs through the read-only passthrough.
#[cfg(feature = "agent-read")]
pub(crate) fn run_usage(args: &UsageArgs, show_sql: bool) -> Result<()> {
    if show_sql {
        line(USAGE_TOTALS_SQL.trim());
        return Ok(());
    }
    run_passthrough(USAGE_TOTALS_SQL, args.format)
}

#[cfg(feature = "agent-read")]
pub(crate) fn run_usage_blocks(
    args: &UsageArgs,
    window_hours: u64,
    active_only: bool,
) -> Result<()> {
    let store = open_ro_store()?;
    let window_ms = (window_hours * 3_600_000) as i64;
    let blocks = store.usage_blocks(window_ms, &usage::UsageQuery::default())?;
    let now = now_ms() as i64;
    let rows: Vec<ident::Row> = blocks
        .iter()
        .filter(|block| !active_only || block.last_ts + window_ms > now)
        .map(|block| {
            serde_json::json!({
                "window_start": block.window_start,
                "first_ts": block.first_ts,
                "last_ts": block.last_ts,
                "calls": block.calls,
                "total_tokens": block.total_tokens,
                "is_gap": block.is_gap,
                "is_active": !block.is_gap && block.last_ts + window_ms > now,
            })
        })
        .collect();
    emit_json_rows(&rows, args.format);
    if let Some(ceiling) = usage::p90_ceiling(&blocks) {
        line(&serde_json::json!({ "p90_token_ceiling": ceiling }).to_string());
    }
    Ok(())
}

#[cfg(feature = "agent-read")]
pub(crate) fn run_usage_burn_rate(args: &UsageArgs, window_minutes: u64) -> Result<()> {
    let store = open_ro_store()?;
    let filter = usage::UsageQuery {
        since: Some(now_ms().saturating_sub(window_minutes * 60_000)),
        ..Default::default()
    };
    emit_json_rows(&store.usage_burn_rate(&filter)?, args.format);
    Ok(())
}

#[cfg(feature = "agent-read")]
pub(crate) fn run_price(cmd: PriceCmd) -> Result<()> {
    let store = open_store()?;
    match cmd {
        PriceCmd::List => {
            emit_json_rows(&store.price_list()?, QueryFormat::Ndjson);
            Ok(())
        }
        PriceCmd::Set {
            model,
            input_per_mtok,
            output_per_mtok,
            cache_write_5m_per_mtok,
            cache_write_1h_per_mtok,
            cache_read_per_mtok,
            source,
        } => {
            store.price_set(&usage::ModelPrice {
                model: &model,
                input_per_mtok,
                output_per_mtok,
                cache_write_5m_per_mtok,
                cache_write_1h_per_mtok,
                cache_read_per_mtok,
                source: &source,
            })?;
            line(&format!("priced {model}"));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::testkit::{route_with, temp_mail_dir};
    use crate::cli::write_line;
    use crate::{AgentSessionGraphFormat, Cli, SubCmd};
    use boop::{
        AgentRuntimeRow, AgentSummary, AgentSummaryActivity, AgentSummaryAgent, MailboxCounts,
        ProcessLiveness, RuntimeLiveness, TmuxLiveness, WorktreeCoordinates,
    };
    use clap::{CommandFactory, Parser};

    /// RECEIPT (instant-focused-family-cli): Instant's argv reaches the graph query unchanged.
    #[test]
    fn public_agent_sessions_accepts_focused_family_filters() {
        let cli = Cli::try_parse_from([
            "boop",
            "agent",
            "sessions",
            "--history",
            "--tmux",
            "sprefa-5",
            "--history-since-ts",
            "1735689600000",
            "--format",
            "json",
        ])
        .expect("Instant focused-family command parses");
        let SubCmd::Agent {
            cmd:
                AgentSummaryCmd::Sessions {
                    cwd,
                    history,
                    tmux,
                    history_since_ts,
                    format: AgentSessionGraphFormat::Json,
                    ..
                },
        } = cli.command
        else {
            panic!("expected public agent sessions command");
        };

        let query = agent_session_graph_query(cwd, history, tmux, history_since_ts);
        assert_eq!(query.tmux.as_deref(), Some("sprefa-5"));
        assert_eq!(query.history_since_ts, Some(1_735_689_600_000));
        assert!(query.include_history);

        let mut command = Cli::command();
        let sessions = command
            .find_subcommand_mut("agent")
            .expect("agent command")
            .find_subcommand_mut("sessions")
            .expect("sessions command");
        let help = sessions.render_long_help().to_string();
        assert!(help.contains("--tmux <TMUX>"), "sessions help:\n{help}");
        assert!(
            help.contains("--history-since-ts <HISTORY_SINCE_TS>"),
            "sessions help:\n{help}"
        );
    }

    #[test]
    fn agent_summary_text_fixture_has_fixed_columns() {
        let summary = AgentSummary {
            schema_version: 1,
            active_agents: 1,
            agents: vec![AgentSummaryAgent {
                runtime: AgentRuntimeRow {
                    lane: "lane-a".into(),
                    trace: Some("trace-a".into()),
                    root_session: None,
                    session: Some("session-a".into()),
                    parent: None,
                    route: None,
                    cwd: None,
                    tmux_target: None,
                    tmux_pane: None,
                    pid: None,
                    reported_status: None,
                    liveness: RuntimeLiveness {
                        tmux: TmuxLiveness::Live,
                        process: ProcessLiveness::Unknown,
                    },
                    completion: None,
                    mailbox: MailboxCounts {
                        inbox: 2,
                        outbox: 3,
                        unacknowledged: 1,
                    },
                    worktree: WorktreeCoordinates::default(),
                    diagnostics: Vec::new(),
                },
                activity: AgentSummaryActivity {
                    user: 4,
                    assistant: 5,
                    tool_call: 6,
                    total: 15,
                    calls: 7,
                    input_tokens: 8,
                    output_tokens: 9,
                    cache_create_5m_tokens: 10,
                    cache_create_1h_tokens: 11,
                    cache_read_tokens: 12,
                    first_activity_ts: None,
                    last_activity_ts: None,
                    tool_result_availability: boop::ToolResultAvailability::Unavailable,
                },
            }],
        };
        let rendered = agent_summary_text(&summary);
        assert_eq!(
            rendered,
            "schema_version\t1\nactive_agents\t1\nlane\ttrace\troot_session\tsession\tparent\troute\tcwd\ttmux_target\ttmux_pane\tpid\treported_status\ttmux_liveness\tprocess_liveness\tcompletion\tinbox\toutbox\tunacknowledged\tworktree_route_cwd\tworktree_process_cwd\tdiagnostics\tuser\tassistant\ttool_call\ttotal\tcalls\tinput_tokens\toutput_tokens\tcache_create_5m_tokens\tcache_create_1h_tokens\tcache_read_tokens\nlane-a\ttrace-a\t-\tsession-a\t-\tnull\t-\t-\t-\t-\t-\tlive\tunknown\tnull\t2\t3\t1\t-\t-\t[]\t4\t5\t6\t15\t7\t8\t9\t10\t11\t12"
        );
        let mut output = Vec::new();
        write_line(&mut output, &rendered).expect("write summary output");
        assert_eq!(output, format!("{rendered}\n").as_bytes());
        assert!(!output.ends_with(b"\n\n"));
    }

    fn session_with_cwd(cwd: Option<&str>) -> boop::harness::SessionRef {
        boop::harness::SessionRef {
            harness: "opencode",
            session_id: "ses-1".into(),
            nickname: "ses-1".into(),
            path: std::path::PathBuf::from("/tmp/x.jsonl"),
            cwd: cwd.map(str::to_owned),
            git_branch: None,
            modified_ms: 0,
            size: 0,
            tmux: None,
            tmux_socket: None,
            parent: None,
        }
    }

    #[test]
    fn none_cwd_on_both_sides_is_not_a_route_match() {
        let mut route = route_with(None);
        route.cwd = None;
        assert!(!session_matches_route(&route, &session_with_cwd(None)));
    }

    #[test]
    fn shared_concrete_cwd_matches_and_none_session_cwd_does_not() {
        let mut route = route_with(None);
        route.cwd = Some("/repo/wt".into());
        assert!(session_matches_route(
            &route,
            &session_with_cwd(Some("/repo/wt"))
        ));
        assert!(!session_matches_route(&route, &session_with_cwd(None)));
    }

    #[test]
    fn session_id_match_needs_no_cwd() {
        let mut route = route_with(None);
        route.session_id = Some("ses-1".into());
        route.cwd = None;
        assert!(session_matches_route(&route, &session_with_cwd(None)));
    }

    /// RECEIPT (boop-db-readonly-open): a store opened `SQLITE_OPEN_READ_ONLY`
    /// still answers `query_sessions`, the shape every converted `db` verb needs.
    #[test]
    fn a_read_verb_succeeds_against_a_readonly_opened_store() {
        let path = temp_mail_dir().join("ro.db");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        ident::Store::open(path.clone()).unwrap();
        let store = ident::Store::open_readonly(path).unwrap();
        assert!(store.query_sessions(None, None).unwrap().is_empty());
    }
}
