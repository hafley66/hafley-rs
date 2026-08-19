//! `boop`: the cross-harness agent-event reader, 1-1 with `bus` plus the four
//! verbs `bus` cannot do (read what an agent did, and measure what its
//! processes cost). The CLI routes to layers 0-3; it contains no `match` on
//! harness id and no direct `Command::new("tmux")` beyond the layer-1 helpers.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use boop::bus::Route;
use boop::mailwait::Watch;
use boop::proc::ProcReader;
use boop::registry::Registry;
use boop::supervise::ParentDeathPolicy;
use boop::{bus, config, ident, identity, lane, mailwait, proc, tmux};

mod cli;

use cli::db::{
    open_ro_store, open_store, resolve_harness, run_chat_query, run_db, run_follow, run_harnesses,
    run_passthrough, run_public_agent_command, run_query, run_sessions, run_sync_all, run_tail,
    sync_all, ChatQueryOptions, SyncLiveness,
};
use cli::mail::{
    all_messages, report_inbox_hooks, run_hail, run_inbox, run_list, run_tell_children,
    run_tell_parent, write_inbox_hooks,
};
use cli::{
    append_ack, append_message, doctrine, line, mail_dir, now_ms, pad, route_to_json, write_route,
    CONCATMAP_EXAMPLES,
};

#[derive(Parser)]
#[command(
    name = "boop",
    version = boop::BUILD,
    about = "Cross-harness agent transcript reader: drive agents with `beep`, read what they did with `db`",
    after_help = doctrine()
)]
struct Cli {
    #[command(subcommand)]
    command: SubCmd,
}

#[derive(Subcommand)]
enum SubCmd {
    /// Drive agents: harnesses, lanes, mail, processes.
    Beep {
        #[command(subcommand)]
        cmd: BeepCmd,
    },
    /// Run raw SQL read-only against the store (the default `db` form), or
    /// read/count what agents did through a `db` subcommand.
    #[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
    Db {
        /// The SQL to run against ~/.agent/boop.db.
        #[arg(value_name = "SQL")]
        sql: Option<String>,
        /// Output format for the SQL passthrough.
        #[arg(long, value_enum)]
        format: Option<QueryFormat>,
        #[command(subcommand)]
        cmd: Option<DbCmd>,
    },
    /// What just went wrong: recent WARN/ERROR across the lane trails and the
    /// store's error events, grouped by lane.
    Debug {
        /// Window to read back, as `Ns`, `Nm`, `Nh` or a count of seconds.
        #[arg(long, default_value = "2m")]
        since: String,
        /// One lane only.
        #[arg(long)]
        lane: Option<String>,
        /// One JSON array instead of the grouped text.
        #[arg(long)]
        json: bool,
    },
    /// Freshly synchronize and summarize Boop agent/runtime/activity facts.
    #[cfg(feature = "agent-read")]
    Agent {
        #[command(subcommand)]
        cmd: AgentSummaryCmd,
    },
    /// Refinement loop: map each new (assistant, user) contact pair through
    /// a model pass and write the rewrite per turn. For a resident DL6
    /// coroutine, use `boop host chat`.
    #[command(after_help = CONCATMAP_EXAMPLES)]
    Concatmap {
        /// Prompt template file; substitutes {{mode}}, {{ai_text}} (the
        /// assistant turn(s) before the user turn), {{user_text}}. Optional
        /// under a rules `window` (the SQL's `text` column ships verbatim).
        #[arg(long)]
        template: Option<PathBuf>,
        /// The mode word substituted into the template (compiled bundling).
        #[arg(long)]
        mode: Option<String>,
        /// The one-shot model id, in the harness's own flag spelling.
        #[arg(long, conflicts_with = "preset")]
        model: Option<String>,
        /// Model preset resolving through boop/config.json, as lane create.
        #[arg(long)]
        preset: Option<String>,
        /// Loop-owned memory: cursor file (last store ts seen; first run
        /// seeds at the newest ts) and done/ markers; chat feed's cwd too.
        #[arg(long)]
        state: PathBuf,
        /// The boop store to read turns from (defaults to the resident store).
        #[arg(long)]
        store: Option<PathBuf>,
        /// Seconds between turn queries.
        #[arg(long, default_value_t = 5)]
        poll_secs: u64,
        /// Seed the cursor at 0 so an existing conversation maps in full.
        /// Default is tail-only (seed at the newest store ts).
        #[arg(long, conflicts_with = "cursor")]
        from_start: bool,
        /// An explicit starting cursor ts (ms); `--from-start` is `--cursor 0`.
        #[arg(long)]
        cursor: Option<i64>,
        /// Rules json naming feed {"oneshot"|"chat"} plus goal, bundle
        /// {"pair"|"run"}, coalesce, references. Absent = oneshot/pair.
        #[arg(long)]
        rules: Option<PathBuf>,
        /// Map one conversation only.
        #[arg(long)]
        session: Option<String>,
        /// Map the caller's own session (the `whoami` ladder resolves it).
        #[arg(long, conflicts_with = "session")]
        me: bool,
    },
    /// Typed stdin/stdout host boundary for compiled DL6 programs.
    Host {
        #[command(subcommand)]
        cmd: HostCmd,
    },
    /// Mail the caller's own parent. The identity ladder names the sender and
    /// the registered parent edge names the recipient, so neither is spelled.
    TellParent {
        /// What the row says it is. `yield` carries a default body.
        #[arg(long, default_value = "note", value_parser = ["completion", "yield", "note"])]
        kind: String,
        /// The message. Required for every kind but `yield`.
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Mail one body to every live child of the caller, resolved from the same
    /// parent edges, with a landed/dead line per target.
    TellChildren {
        /// The message every child gets.
        #[arg(long)]
        body: String,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Report the caller's own identity and the rung that resolved it.
    Whoami {
        #[arg(long)]
        json: bool,
    },
    /// Block until mail lands: the reply to <id>, or the next unread row
    /// addressed to you with --me. Every exit prints the next command to run.
    Wait {
        /// The id `boop beep hail` printed. Omit it and pass --me instead.
        #[arg(value_name = "MESSAGE-ID", required_unless_present = "me")]
        id: Option<String>,
        /// Wait for the next unread mail addressed to the caller.
        #[arg(long, conflicts_with = "id")]
        me: bool,
        /// Whose inbox to watch, when the whoami ladder cannot say.
        #[arg(long = "as", value_name = "NAME")]
        as_name: Option<String>,
        /// Seconds to block before exiting 124.
        #[arg(long, default_value_t = mailwait::DEFAULT_TIMEOUT_SECS)]
        wait_timeout: u64,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Mail a claude coordinator reads at a turn boundary: the hook inbox.
    Inbox {
        #[command(subcommand)]
        cmd: InboxCmd,
    },
    /// Register this Codex pane, or act on the caller's own conversation.
    #[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
    Me {
        /// Registry name; defaults to codex-<pane id>.
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
        #[command(subcommand)]
        cmd: Option<MeCmd>,
    },
    /// Inspect the boop configuration the CLI reads.
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
    /// List registered harness adapters, one per line. (pass 1)
    #[command(hide = true)]
    Harnesses,
    /// List on-disk sessions, newest last. (pass 1)
    #[command(hide = true)]
    Sessions {
        /// Only sessions from this harness (its stable id).
        #[arg(long)]
        harness: Option<String>,
    },
    /// Tail one session's events from a byte offset. (pass 1)
    #[command(hide = true)]
    Tail {
        /// The session id to read.
        session_id: String,
        /// Byte offset to start from. Defaults to 0.
        #[arg(long)]
        from: Option<u64>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Stream turns across sessions, filtered from the db. (pass 4)
    #[command(hide = true)]
    Events {
        #[command(flatten)]
        query: QueryArgs,
    },
    /// List lanes and messages like `bus list`.
    #[command(hide = true)]
    List {
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Measure per-lane pid, rss, cpu, uptime, child count.
    #[command(hide = true)]
    Measure {
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Spawn a lane: tmux new-session + mailbox + registry route.
    #[command(hide = true)]
    Dispatch {
        #[arg(long)]
        to: String,
        #[arg(long)]
        cwd: String,
        #[arg(long)]
        cmd: String,
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        harness: Option<String>,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        mode: Option<String>,
        #[arg(long)]
        tmux: Option<String>,
        #[arg(long)]
        socket: Option<String>,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        r#ref: Option<String>,
        #[arg(long)]
        goal: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
        #[arg(long, default_value_t = 3)]
        resolve_wait: u64,
        /// Spawn in the main tree instead of creating a worktree.
        #[arg(long)]
        main_tree: bool,
        #[arg(long)]
        base_sha: Option<String>,
    },
    /// Resolve a lane's harness session id into its registry route.
    #[command(hide = true)]
    Resolve {
        #[arg(long)]
        to: String,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Queue a message and inject it into a live pane.
    #[command(hide = true)]
    Hail {
        #[arg(long)]
        to: String,
        #[arg(long)]
        body: String,
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        box_: Option<String>,
        #[arg(long)]
        socket: Option<String>,
        /// Send, then block for the reply exactly as `boop wait <id>` does.
        #[arg(long, value_name = "SECS")]
        wait_timeout: Option<u64>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Acknowledge unacked lanes via cass and stamp token usage.
    #[command(hide = true)]
    Sweep {
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        box_: Option<String>,
        #[arg(long)]
        close_routeless: bool,
        #[arg(long, default_value_t = 7)]
        max_age_days: u64,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Register and spawn a lane (the first-contact verb).
    #[command(hide = true)]
    Lane {
        #[arg(long)]
        name: String,
        #[arg(long)]
        cwd: String,
        #[arg(long)]
        harness: Option<String>,
        #[arg(long)]
        brief: Option<PathBuf>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long, conflicts_with = "model")]
        preset: Option<String>,
        #[arg(long)]
        tmux: Option<String>,
        #[arg(long)]
        parent: Option<String>,
        /// New branch name; with `--base-sha`, spawns in a worktree instead
        /// of `--cwd` directly.
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        base_sha: Option<String>,
        /// tmux socket to spawn on; a throwaway socket for tests, `None` for
        /// the default server.
        #[arg(long)]
        socket: Option<String>,
        #[arg(long)]
        goal: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Register an existing interactive pane as a coordinator route; never
    /// spawns. A claude session also gets the hook inbox, and reads its mail at
    /// the next turn boundary; every other harness has it typed into its pane.
    #[command(hide = true)]
    Adopt {
        #[arg(long)]
        name: String,
        #[arg(long)]
        tmux: String,
        /// Keep pane injection: do not install the hook inbox for a claude
        /// coordinator.
        #[arg(long)]
        no_hooks: bool,
        /// Take the hook inbox back out of the project settings and leave the
        /// route alone. The pane is not checked, so a dead one is fine;
        /// `boop inbox hooks --uninstall` is the same edit without a route.
        #[arg(long)]
        uninstall_hooks: bool,
        #[arg(long)]
        harness: Option<String>,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long)]
        cwd: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        mode: Option<String>,
        /// The lane that summoned this one.
        #[arg(long)]
        parent: Option<String>,
        #[arg(long)]
        goal: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Drop lanes whose tmux sessions are gone. Refuses when tmux is
    /// unreachable because it cannot tell live from dead.
    #[command(hide = true)]
    Prune {
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Project sessions into NDJSON chat-repr turns (the zipf door).
    #[command(hide = true)]
    Chat {
        #[command(flatten)]
        query: QueryArgs,
        /// Project every session the registry knows.
        #[arg(long)]
        all: bool,
        /// Tail new turns from the db, one NDJSON line per new turn.
        #[arg(long)]
        follow: bool,
    },
    /// Tail every harness forward from stored offsets into the db.
    #[command(hide = true)]
    Sync {
        /// Drop every stored row and re-project every transcript from byte 0.
        /// Required once to move a store off pre-dense turn ordinals.
        #[arg(long)]
        rebuild: bool,
    },
    /// Stream new facts into the db on a coarse poll (idle near-zero CPU).
    #[command(hide = true)]
    Follow {},
}

/// The shared read filter, used by `chat` and `events`.
#[derive(clap::Args, Clone, Default)]
struct QueryArgs {
    #[arg(long)]
    harness: Option<String>,
    #[arg(long)]
    session: Option<String>,
    #[arg(long)]
    role: Option<String>,
    #[arg(long)]
    since: Option<u64>,
    #[arg(long)]
    until: Option<u64>,
    #[arg(long)]
    turn_from: Option<u64>,
    #[arg(long)]
    turn_to: Option<u64>,
    #[arg(long)]
    path: Option<String>,
    #[arg(long)]
    limit: Option<u64>,
    #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
    format: QueryFormat,
}

#[derive(Clone, Copy, ValueEnum, Default)]
enum QueryFormat {
    #[default]
    Ndjson,
    Text,
}

#[cfg(feature = "agent-read")]
#[derive(Clone, Copy, ValueEnum)]
enum AgentSummaryFormat {
    Text,
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Clone, Copy, ValueEnum, Default)]
enum PstreeFormat {
    #[default]
    Text,
    Ndjson,
}

#[derive(Subcommand)]
enum ConfigCmd {
    /// Print the resolved config path.
    Path,
    /// Print the loaded config as pretty JSON, including the defaults a
    /// missing file produces.
    Show,
    /// One row per model preset: name, model, variant, the harness the model
    /// spelling names, and which row `default-model-preset` points at.
    Presets,
}

/// Whether this invocation is asking for help, whatever verb it names.
fn help_wanted() -> bool {
    std::env::args().any(|argument| argument == "--help" || argument == "-h")
}

/// `boop debug`: the WARN/ERROR window, trail plus store.
fn run_debug(since: &str, lane: Option<&str>, json: bool) -> Result<()> {
    let window = boop::debug::parse_window(since)?;
    let since_ms = now_ms().saturating_sub(window.as_millis() as u64);
    let root = boop::trail::lanes_root()?;
    let mut alerts = boop::debug::trail_alerts(&root, since_ms, lane);
    match open_ro_store().and_then(|store| boop::debug::store_alerts(&store, since_ms, lane)) {
        Ok(rows) => alerts.extend(rows),
        Err(error) => warn!(error = %error, "trace error events unreadable"),
    }
    alerts.sort_by(|left, right| {
        left.lane
            .cmp(&right.lane)
            .then(left.at_ms.cmp(&right.at_ms))
    });
    match json {
        true => line(&serde_json::to_string_pretty(&boop::debug::as_json(
            &alerts,
        ))?),
        false => line(&boop::debug::report(&alerts, window)),
    }
    Ok(())
}

fn run_host(cmd: HostCmd) -> Result<()> {
    match cmd {
        HostCmd::Chat => {
            let response =
                match serde_json::from_reader::<_, boop::host::ChatRequest>(std::io::stdin()) {
                    Ok(request) => boop::host::run_chat(request),
                    Err(error) => boop::host::ChatResponse::Failed {
                        outcome: "failed",
                        detail: format!("read host chat JSON: {error}"),
                    },
                };
            println!("{}", serde_json::to_string(&response)?);
            Ok(())
        }
    }
}

fn main() -> Result<()> {
    // Only a help invocation pays for the trail read the banner needs.
    if help_wanted() {
        if let Some(banner) = boop::debug::help_banner(now_ms()) {
            line(&banner);
        }
    }
    let cli = Cli::parse();
    init_tracing(supervised_lane(&cli.command))?;
    let registry = Registry::discover();
    let needs_startup_sync = command_needs_startup_sync(&cli.command);
    run_with_startup_sync(
        needs_startup_sync,
        || sync_before_local_command(&registry),
        || match cli.command {
            SubCmd::Harnesses => run_harnesses(&registry),
            SubCmd::Sessions { harness } => run_sessions(&registry, harness.as_deref()),
            SubCmd::Tail {
                session_id,
                from,
                format,
            } => run_tail(&registry, &session_id, from.unwrap_or(0), format),
            SubCmd::Events { query } => run_query(&query),
            SubCmd::Sync { rebuild } => run_sync_all(&registry, rebuild),
            SubCmd::Debug { since, lane, json } => run_debug(&since, lane.as_deref(), json),
            #[cfg(feature = "agent-read")]
            SubCmd::Agent { cmd } => run_public_agent_command(cmd),
            SubCmd::Follow {} => run_follow(&registry),
            SubCmd::Chat { query, all, follow } => {
                run_chat_query(&query, ChatQueryOptions { all, follow })
            }
            SubCmd::List {
                agent,
                all,
                mail_dir,
            } => run_list(mail_dir.as_deref(), agent.as_deref(), all),
            SubCmd::Measure { mail_dir } => run_measure(mail_dir.as_deref()),
            SubCmd::Dispatch {
                to,
                cwd,
                cmd,
                from,
                harness,
                session_id,
                model,
                mode,
                tmux,
                socket,
                body,
                r#ref,
                goal,
                mail_dir,
                resolve_wait,
                main_tree,
                base_sha,
            } => run_dispatch(
                &registry,
                DispatchArgs {
                    to,
                    cwd,
                    cmd,
                    from,
                    harness,
                    session_id,
                    model,
                    mode,
                    tmux,
                    socket,
                    body,
                    r#ref,
                    goal,
                    mail_dir,
                    resolve_wait,
                    main_tree,
                    base_sha,
                    branch: None,
                    worktree_dir: None,
                    parent: None,
                    on_exit: None,
                    warm_start: true,
                    variant: None,
                },
            ),
            SubCmd::Resolve { to, mail_dir } => run_resolve(&to, mail_dir.as_deref()),
            SubCmd::Hail {
                to,
                body,
                from,
                kind,
                box_,
                socket,
                wait_timeout,
                mail_dir,
            } => run_hail(
                &registry,
                &to,
                &body,
                from.as_deref(),
                kind.as_deref(),
                box_.as_deref(),
                socket.as_deref(),
                wait_timeout,
                mail_dir.as_deref(),
            ),
            SubCmd::Sweep {
                agent,
                box_,
                close_routeless,
                max_age_days,
                mail_dir,
            } => run_sweep(
                mail_dir.as_deref(),
                box_.as_deref(),
                agent.as_deref(),
                close_routeless,
                max_age_days,
            ),
            SubCmd::Lane {
                name,
                cwd,
                harness,
                brief,
                model,
                preset,
                tmux,
                parent,
                branch,
                base_sha,
                socket,
                goal,
                mail_dir,
                dry_run,
            } => run_lane(
                &registry,
                LaneArgs {
                    name: Some(name),
                    cwd: Some(cwd),
                    harness,
                    brief,
                    model,
                    preset,
                    variant: None,
                    tmux,
                    parent,
                    branch,
                    base_sha,
                    socket,
                    goal,
                    mood: None,
                    trace: None,
                    no_start: false,
                    mail_dir,
                    dry_run,
                    wait: false,
                    wait_timeout: 0,
                    reclaim: false,
                },
            ),
            SubCmd::Adopt {
                name,
                tmux,
                no_hooks,
                uninstall_hooks,
                harness,
                session_id,
                cwd,
                model,
                mode,
                parent,
                goal,
                mail_dir,
                // An adopted pane is an interactive session with no lane supervisor
                // polling its mailbox; `coordinator` makes hail deliver by pane injection.
            } => run_adopt(
                &name,
                "coordinator",
                &tmux,
                harness.as_deref(),
                session_id.as_deref(),
                cwd.as_deref(),
                model.as_deref(),
                mode.as_deref(),
                parent.as_deref(),
                goal.as_deref(),
                mail_dir.as_deref(),
                HookWiring {
                    no_hooks,
                    uninstall: uninstall_hooks,
                },
            ),
            SubCmd::Prune { mail_dir } => run_prune(mail_dir.as_deref()),
            SubCmd::Beep { cmd } => run_beep(&registry, cmd),
            SubCmd::Db { sql, format, cmd } => match cmd {
                Some(cmd) => run_db(&registry, cmd),
                None => match sql {
                    Some(sql) => run_passthrough(&sql, format.unwrap_or_default()),
                    None => anyhow::bail!(
                        "boop db needs a SQL string or a subcommand; see `boop db --help`"
                    ),
                },
            },
            SubCmd::Concatmap {
                template,
                mode,
                model,
                preset,
                state,
                store,
                poll_secs,
                from_start,
                cursor,
                rules,
                session,
                me,
            } => {
                // Common model-selection subset: explicit model wins, preset
                // resolves through config; flash4 is the standing default.
                let config_path = config::default_path()?;
                let model = match (model, preset) {
                    (Some(model), _) => model,
                    (None, Some(preset)) => config::resolve_model(&preset, &config_path)?,
                    (None, None) => config::resolve_model("flash4", &config_path)?,
                };
                let formula = match &rules {
                    Some(path) => boop::concatmap::Formula::load(path)?,
                    None => boop::concatmap::Formula::oneshot(),
                };
                let template = match &template {
                    Some(path) => {
                        Some(boop::concatmap::expand_env(&std::fs::read_to_string(path)?))
                    }
                    None => None,
                };
                if formula.window.is_none() {
                    anyhow::ensure!(
                    template.is_some() && mode.is_some(),
                    "compiled bundling needs --template and --mode; or pass --rules with a window SQL"
                );
                }
                let session = match (session, me) {
                    (Some(session), _) => Some(session),
                    (None, true) => {
                        let routes = bus::read_routes(&mail_dir(None)?).unwrap_or_default();
                        let identity = identity::resolve_with(&registry, &routes)?;
                        Some(identity.session.context(
                        "--me found no caller session (no BOOP_SESSION, no pane or process rung); pass --session <id>",
                    )?)
                    }
                    (None, false) => anyhow::bail!(
                    "name the conversation to map: --session <id>, or --me to take the caller's own"
                ),
                };
                boop::concatmap::run(boop::concatmap::Args {
                    template,
                    mode,
                    model,
                    state_dir: state,
                    store_path: store,
                    poll: std::time::Duration::from_secs(poll_secs),
                    from_start,
                    cursor,
                    formula,
                    session,
                })
            }
            SubCmd::Host { cmd } => run_host(cmd),
            SubCmd::Wait {
                id,
                me,
                as_name,
                wait_timeout,
                mail_dir,
            } => run_wait(
                id.as_deref(),
                me,
                as_name.as_deref(),
                wait_timeout,
                mail_dir.as_deref(),
            ),
            SubCmd::TellParent {
                kind,
                body,
                mail_dir,
            } => run_tell_parent(&registry, &kind, body.as_deref(), mail_dir.as_deref()),
            SubCmd::TellChildren { body, mail_dir } => {
                run_tell_children(&registry, &body, mail_dir.as_deref())
            }
            SubCmd::Whoami { json } => run_whoami(json),
            SubCmd::Inbox { cmd } => run_inbox(cmd),
            SubCmd::Me {
                name,
                mail_dir,
                cmd,
            } => match cmd {
                Some(MeCmd::Mood {
                    name: mood,
                    clear,
                    as_name,
                }) => run_me_mood(
                    mood.as_deref(),
                    clear,
                    as_name.as_deref(),
                    mail_dir.as_deref(),
                ),
                Some(MeCmd::Favorite { index, note }) => run_me_favorite(index, note.as_deref()),
                None => run_me(name.as_deref(), mail_dir.as_deref()),
            },
            SubCmd::Config { cmd } => run_config(cmd),
        },
    )
}

fn sync_before_local_command(registry: &Registry) -> Result<()> {
    sync_all(registry, false, false, SyncLiveness::TranscriptOnly)
}

/// Verbs that read `agent_*` rows. A registry, mailbox, tmux or live-process
/// verb stays off: a cold cursor re-parses every transcript root from offset 0.
fn command_needs_startup_sync(command: &SubCmd) -> bool {
    matches!(
        command,
        SubCmd::Db { cmd: None, .. }
            | SubCmd::Db {
                cmd: Some(DbCmd::SyncCursor { .. }),
                ..
            }
            | SubCmd::Db {
                cmd: Some(DbCmd::Status { .. }),
                ..
            }
            | SubCmd::Db {
                cmd: Some(DbCmd::Session { .. }),
                ..
            }
            | SubCmd::Db {
                cmd: Some(DbCmd::Turn { .. }),
                ..
            }
            | SubCmd::Db {
                cmd: Some(DbCmd::Chat { .. }),
                ..
            }
            | SubCmd::Db {
                cmd: Some(DbCmd::Edge { .. }),
                ..
            }
            | SubCmd::Db {
                cmd: Some(DbCmd::Usage { .. }),
                ..
            }
            | SubCmd::Db {
                cmd: Some(DbCmd::Price { .. }),
                ..
            }
            | SubCmd::Db {
                cmd: Some(DbCmd::Favorite { .. }),
                ..
            }
            | SubCmd::Db {
                cmd: Some(DbCmd::Touch { .. }),
                ..
            }
            | SubCmd::Db {
                cmd: Some(DbCmd::Command { .. }),
                ..
            }
            | SubCmd::Db {
                cmd: Some(DbCmd::Fetch { .. }),
                ..
            }
            | SubCmd::Db {
                cmd: Some(DbCmd::Skill { .. }),
                ..
            }
            | SubCmd::Db {
                cmd: Some(DbCmd::Pr { .. }),
                ..
            }
            | SubCmd::Db {
                cmd: Some(DbCmd::Span { .. }),
                ..
            }
            | SubCmd::Db {
                cmd: Some(DbCmd::AgentSummary { .. }),
                ..
            }
            | SubCmd::Events { .. }
            | SubCmd::Chat { .. }
            | SubCmd::Agent { .. }
            | SubCmd::Concatmap { .. }
            | SubCmd::Me { .. }
            | SubCmd::Debug { .. }
            | SubCmd::Harnesses
            | SubCmd::Sessions { .. }
            | SubCmd::Tail { .. }
    )
}

fn run_with_startup_sync<T>(
    needs_sync: bool,
    sync: impl FnOnce() -> Result<()>,
    run: impl FnOnce() -> Result<T>,
) -> Result<T> {
    if needs_sync {
        sync()?;
    }
    run()
}

/// Install the one subscriber for the CLI. Libraries emit spans and events
/// only, so embedding `boop` never changes its caller's subscriber.
///
/// A lane supervisor also writes every event to `~/.agent/lanes/<lane>/supervise.log`:
/// the pane it logs to can be killed, and its scrollback goes with it.
fn init_tracing(lane: Option<&str>) -> Result<()> {
    let lane_log = lane.and_then(|lane| boop::trail::open(lane, boop::trail::SUPERVISE_LOG));
    // The file copy carries no escape codes; the pane keeps its colours only
    // when there is no file to share the formatter with.
    let ansi = lane_log.is_none();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_ansi(ansi)
        .with_writer(boop::trail::lane_writer(lane_log))
        .try_init()
        .map_err(|error| anyhow::anyhow!("initialise tracing subscriber: {error}"))
}

/// The lane this invocation supervises, which is the only verb whose whole run
/// belongs in one lane's trail.
fn supervised_lane(command: &SubCmd) -> Option<&str> {
    match command {
        SubCmd::Beep {
            cmd: BeepCmd::Lane {
                cmd: LaneCmd::Run { lane, .. },
            },
        } => Some(lane),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// measure (layer 0)
// ---------------------------------------------------------------------------

fn run_measure(mail_dir_arg: Option<&Path>) -> Result<()> {
    let snapshot = proc::SysinfoSnapshot::capture()?;
    run_measure_with(mail_dir_arg, &snapshot)
}

/// Takes the `ProcReader` seam rather than the concrete snapshot, so a fake
/// reader can drive this without a real process tree.
fn run_measure_with(mail_dir_arg: Option<&Path>, reader: &dyn proc::ProcReader) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    line("lane\tpid\trss_kb\tcpu_pct\tuptime_sec\tchildren");
    for (name, route) in &routes {
        let pane_pid = route
            .tmux
            .as_deref()
            .and_then(|target| tmux::mux().pane_pid(None, target))
            .unwrap_or(0);
        match proc::tree_sum_of(reader, pane_pid) {
            Some(sum) => {
                let now = now_unix_secs();
                let uptime = proc::uptime_secs(sum.start_time_secs, now);
                line(&format!(
                    "{}\t{}\t{}\t{:.1}\t{}\t{}",
                    name,
                    pane_pid,
                    sum.rss_bytes / 1024,
                    sum.cpu_percent,
                    uptime,
                    reader.descendant_count(pane_pid),
                ));
            }
            None => println!("{}\t{}\t-\t-\t-\t-", name, pane_pid),
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// dispatch (layer 1 + bus)
// ---------------------------------------------------------------------------

struct DispatchArgs {
    to: String,
    cwd: String,
    cmd: String,
    from: Option<String>,
    harness: Option<String>,
    session_id: Option<String>,
    model: Option<String>,
    mode: Option<String>,
    tmux: Option<String>,
    socket: Option<String>,
    body: Option<String>,
    r#ref: Option<String>,
    mail_dir: Option<PathBuf>,
    resolve_wait: u64,
    main_tree: bool,
    base_sha: Option<String>,
    /// opencode reasoning-effort variant, threaded from `lane create`.
    variant: Option<String>,
    /// Overrides the branch name derived from `tmux`/`to`; `lane create`
    /// sets this from its own `--branch` flag.
    branch: Option<String>,
    /// The worktree to create; `None` spawns in `cwd` (`main_tree` decides
    /// whether that's a fast-forward check or a plain directory).
    worktree_dir: Option<PathBuf>,
    /// The lane that summoned this one; written to the route's `parent`.
    parent: Option<String>,
    /// What the lane is running toward; written to the route and dispatch mail.
    goal: Option<String>,
    /// Shell appended after the harness command; `lane create --parent` and
    /// foreground `lane create --wait` compose the completion hail here.
    on_exit: Option<String>,
    /// Run the repo's `boop-start` recipe in a new worktree before spawning.
    warm_start: bool,
}

fn run_dispatch(registry: &Registry, args: DispatchArgs) -> Result<()> {
    let adapter = resolve_dispatch_harness(registry, args.harness.as_deref())?;
    let harness_id = adapter.id().to_owned();
    info!(
        lane = args.to,
        harness = harness_id,
        model = args.model.as_deref().unwrap_or_default(),
        cwd = args.cwd,
        tmux_target = args.tmux.as_deref().unwrap_or_default(),
        "lane dispatch starting"
    );
    let branch = args
        .branch
        .clone()
        .unwrap_or_else(|| args.tmux.clone().unwrap_or_else(|| args.to.clone()));
    let base_sha = match &args.base_sha {
        Some(sha) => sha.clone(),
        None => git_head(&args.cwd)?.unwrap_or_else(|| "HEAD".into()),
    };
    let dir = mail_dir(args.mail_dir.as_deref())?;
    let mut body = args.body.clone().unwrap_or_else(|| args.cmd.clone());
    // A dispatch's goal rides the route's `goal` field; embed it in the mail
    // row body too so history states the goal without a registry lookup.
    if let Some(goal) = &args.goal {
        body = format!("{body}\n[goal] {goal}");
    }

    let message = bus::Message {
        id: bus::mint_id(),
        from: args.from.clone().unwrap_or_else(|| "coordinator".into()),
        to: args.to.clone(),
        from_timestamp: bus::now_iso(),
        to_timestamp: None,
        kind: "dispatch".into(),
        reply_to: None,
        body,
        r#ref: args.r#ref.clone(),
        rc: None,
        detail: None,
    };

    let spec = boop::harness::SpawnSpec {
        harness: harness_id.clone(),
        branch,
        base_sha,
        main_tree: args.main_tree,
        setup: Vec::new(),
        prompt: args.cmd.clone(),
        resume_session: args.session_id.clone(),
        socket: args.socket.clone(),
        worktree_dir: args.worktree_dir.clone(),
        repo: std::path::PathBuf::from(&args.cwd),
        env_stamp: Some(spawn_env_stamp(
            &args.to,
            &harness_id,
            args.parent.as_deref(),
        )),
        model: args.model.clone(),
        variant: args.variant.clone(),
        on_exit: args.on_exit.clone(),
        tmux: args.tmux.clone(),
        lane: args.to.clone(),
        mail_dir: dir.clone(),
        warm_start: args.warm_start,
    };
    let session = adapter.spawn(&spec)?;

    // The route's cwd is where the harness actually runs (the worktree when
    // one was made): session-id resolution joins opencode.db on directory.
    let route = Route {
        kind: "lane".into(),
        harness: Some(harness_id),
        tmux: session.tmux.clone(),
        cwd: session.cwd.clone().or_else(|| Some(args.cwd.clone())),
        model: args.model.clone(),
        mode: args.mode.clone(),
        session_id: args.session_id.clone(),
        source_path: None,
        parent: args.parent.clone(),
        goal: args.goal.clone(),
        registered_at: Some(bus::now_iso()),
        base_sha: Some(spec.base_sha.clone()),
        worktree_dir: args
            .worktree_dir
            .clone()
            .map(|dir| dir.display().to_string()),
    };
    write_route(&dir, &args.to, route)?;
    append_message(&dir, &message)?;
    info!(
        lane = args.to,
        harness = adapter.id(),
        tmux_target = session.tmux.as_deref().unwrap_or_default(),
        conversation_id = session.session_id,
        conversation_id_kind = "spawn_handle",
        "lane dispatch registered"
    );
    println!(
        "dispatched {} -> {} (tmux {})",
        message.id,
        args.to,
        session.tmux.as_deref().unwrap_or("-")
    );
    std::thread::sleep(std::time::Duration::from_secs(args.resolve_wait));
    Ok(())
}

/// The environment a spawn's command carries: a UTF-8 locale, then the child's
/// own identity. The pane's inherited locale is the tmux server's, not a shell's.
fn spawn_env_stamp(lane_id: &str, harness_id: &str, parent_lane: Option<&str>) -> String {
    format!(
        "{} {}",
        lane::locale_stamp(),
        identity::child_stamp(lane_id, lane_id, harness_id, parent_lane)
    )
}

/// The registered harness adapter for a dispatched `--harness`. A named
/// harness must resolve exactly; an unnamed one takes the first registered
/// adapter. A named harness resolving to a different harness is a capability
/// lie, so an unregistered name is a hard error that lists the registered set.
fn resolve_dispatch_harness<'a>(
    registry: &'a Registry,
    id: Option<&str>,
) -> Result<&'a dyn boop::harness::Harness> {
    let Some(id) = id else {
        return registry
            .all()
            .first()
            .map(|boxed| boxed.as_ref())
            .ok_or_else(|| anyhow::anyhow!("no harness registered"));
    };
    match registry.by_id(id) {
        Some(adapter) => Ok(adapter),
        None => {
            let registered = registry
                .all()
                .iter()
                .map(|harness| harness.id())
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::bail!("unregistered harness `{id}`; registered harnesses: {registered}")
        }
    }
}

/// The registered harness adapter for a `--harness` filter, or the first
/// registered one when the id is absent.
fn harness_by_id<'a>(registry: &'a Registry, id: &str) -> Result<&'a dyn boop::harness::Harness> {
    registry
        .by_id(id)
        .or_else(|| registry.all().first().map(|b| b.as_ref()))
        .ok_or_else(|| anyhow::anyhow!("no harness registered"))
}

fn git_head(repo: &str) -> Result<Option<String>> {
    let output = std::process::Command::new("git")
        .args(["-C", repo, "rev-parse", "HEAD"])
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(
        String::from_utf8_lossy(&output.stdout).trim().to_owned(),
    ))
}

// ---------------------------------------------------------------------------
// resolve
// ---------------------------------------------------------------------------

fn run_resolve(to: &str, mail_dir_arg: Option<&Path>) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let route = match routes.get(to) {
        Some(route) => route,
        None => {
            println!("unresolved {to}: no registry route");
            return Ok(());
        }
    };
    if route.session_id.is_some() {
        println!(
            "resolved {to} -> {} (self-reported)",
            route.session_id.as_deref().unwrap()
        );
        return Ok(());
    }
    let harness = route.harness.as_deref().unwrap_or("-");
    let Some(cwd) = route.cwd.as_deref() else {
        println!("unresolved {to}: no cwd in registry route");
        return Ok(());
    };
    match resolve_harness_binary(harness, cwd) {
        Some(session_id) => {
            let mut updated = route.clone();
            updated.session_id = Some(session_id.clone());
            println!("resolved {to} -> {session_id}");
            let path = dir.join("registry.json");
            bus::cas_update_json(&path, |current| {
                current.insert(to.to_owned(), route_to_json(&updated));
                Ok(())
            })?;
            Ok(())
        }
        None => {
            println!("unresolved {to}: no {harness} session for {cwd} yet");
            Ok(())
        }
    }
}

/// Resolve via the instant-harness binary when it exists (the same binary
/// `bus` shells out to); `None` when the binary is absent or finds nothing.
fn resolve_harness_binary(harness: &str, cwd: &str) -> Option<String> {
    let root = dirs::home_dir()?.join("projects/instant");
    let candidates = [
        root.join("src-tauri/target/debug/instant-harness"),
        root.join("src-tauri/target/release/instant-harness"),
    ];
    let binary = candidates.iter().find(|path| path.exists())?;
    let output = Command::new(binary)
        .args(["resolve", "--harness", harness, "--cwd", cwd])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_session_id(&String::from_utf8_lossy(&output.stdout))
}

fn parse_session_id(text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    value
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

/// Drive one lane to completion inside its pane, then exit with the harness's
/// own code so the pane re-raises a true rc.
#[allow(clippy::too_many_arguments)]
fn run_lane_supervisor(
    registry: &Registry,
    lane: &str,
    harness_id: &str,
    brief: &Path,
    model: Option<&str>,
    resume: Option<&str>,
    variant: Option<&str>,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    info!(
        lane,
        harness = harness_id,
        model = model.unwrap_or_default(),
        cwd = %std::env::current_dir().unwrap_or_default().display(),
        resume = resume.unwrap_or_default(),
        variant = variant.unwrap_or_default(),
        "lane supervisor starting"
    );
    let adapter = harness_by_id(registry, harness_id)?;
    let dir = mail_dir(mail_dir_arg)?;
    let cwd = std::env::current_dir().context("read the current directory")?;
    // A respawned lane continues its pinned conversation instead of cold-
    // starting a new one with the full brief.
    let resume = resume
        .map(str::to_owned)
        .or_else(|| boop::supervise::pinned_conversation(&dir, lane));
    let resume = resume.as_deref();
    let spec = boop::channel::ChannelSpec {
        model: model.map(str::to_owned),
        cwd: cwd.clone(),
        resume: resume.map(str::to_owned),
        lane: Some(lane.to_owned()),
    };
    let mut channel = adapter.open_channel(&spec).inspect_err(|error| {
        error!(lane, harness = harness_id, error = %error, "lane channel open failed");
    })?;
    let run = boop::supervise::LaneRun {
        lane: lane.to_owned(),
        // The warm-up's outcome and the setup sentence lead the first turn.
        brief: boop::lane::brief_with_preamble(&dir, lane, brief),
        mail_dir: dir,
        cwd,
        model: model.map(str::to_owned),
        resume: resume.map(str::to_owned),
    };
    // Process-global, so it is armed here and not inside the library call.
    boop::supervise::arm_signal_trail(&run);
    let code = boop::supervise::run(run, channel.as_mut()).inspect_err(|error| {
        error!(lane, harness = harness_id, error = %error, "lane supervisor failed");
    })?;
    info!(
        lane,
        harness = harness_id,
        exit_code = code,
        "lane supervisor finished"
    );
    println!("[boop] lane {lane} finished rc={code}");
    std::process::exit(code);
}

/// Write what the lane was told to do, including the brief bytes as of now:
/// the file on disk is edited afterward and then nothing recovers the text.
#[allow(clippy::too_many_arguments)]
fn record_lane_purpose(
    lane: &str,
    trace: &str,
    harness: &str,
    branch: &str,
    repo: &Path,
    model: Option<&str>,
    parent: Option<&str>,
    goal: Option<&str>,
    brief: &Path,
) {
    let Ok(store) = boop::Store::default_path().and_then(boop::Store::open) else {
        return;
    };
    let spawn = boop::ident::LaneSpawn {
        lane: lane.to_owned(),
        trace: Some(trace.to_owned()),
        harness: Some(harness.to_owned()),
        branch: Some(branch.to_owned()),
        cwd: Some(repo.display().to_string()),
        model: model.map(str::to_owned),
        parent: parent.map(str::to_owned),
        goal: goal.map(str::to_owned),
        brief_path: Some(brief.display().to_string()),
        brief_body: std::fs::read_to_string(brief).ok(),
        ts: boop::channel::now_ms(),
    };
    if let Err(error) = store.record_lane_spawn(&spawn) {
        eprintln!("[boop] lane purpose not recorded: {error}");
    }
    let _ = store.attach_trace(lane, trace, "lane-create", boop::channel::now_ms());
}

/// Set the child's mood at spawn. No `agent_session` row exists yet: the
/// transcript sync writes that later, so the attribute is keyed on the lane
/// name's `dict_session` id, which is the same id `agent_lane` records.
fn record_lane_mood(lane: &str, mood: &str) -> Result<()> {
    let store = boop::Store::open(boop::Store::default_path()?)?;
    store.set_session_mood(lane, mood, boop::channel::now_ms())
}

// ---------------------------------------------------------------------------
// wait
// ---------------------------------------------------------------------------

/// How often the mailbox is re-read. boop carries no file-watch dependency
/// (notify lives in soopy), and a mail wait is measured in minutes.
const WAIT_POLL: std::time::Duration = std::time::Duration::from_secs(1);

fn run_wait(
    id: Option<&str>,
    me: bool,
    as_name: Option<&str>,
    timeout_secs: u64,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let watch = match (id, me) {
        (Some(id), _) => Watch::Reply { id: id.to_owned() },
        (None, _) => Watch::Inbox {
            name: waiting_as(&dir, as_name)?,
        },
    };
    wait_and_exit(&dir, watch, timeout_secs, as_name, mail_dir_arg)
}

/// Whose inbox `--me` watches: the name given, else the identity ladder's lane
/// or session. An unresolved caller is told to name itself, never guessed at.
fn waiting_as(dir: &Path, as_name: Option<&str>) -> Result<String> {
    if let Some(name) = as_name {
        return Ok(name.to_owned());
    }
    let routes = bus::read_routes(dir).unwrap_or_default();
    let identity = identity::resolve(&routes)?;
    identity.lane.or(identity.session).context(
        "boop wait --me cannot tell who you are; pass --as <name> (boop whoami shows the ladder)",
    )
}

/// Block until the watch is satisfied, print what arrived, take delivery of it,
/// and exit. A timeout exits 124 with the re-run line on both streams.
fn wait_and_exit(
    dir: &Path,
    watch: Watch,
    timeout_secs: u64,
    as_name: Option<&str>,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    let command = watch.command(
        timeout_secs,
        as_name,
        mail_dir_arg
            .map(|path| path.display().to_string())
            .as_deref(),
    );
    info!(watching = watch.what(), timeout_secs, "mail wait starting");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        let arrivals = watch.arrivals(&all_messages(dir)?);
        if !arrivals.is_empty() {
            info!(
                watching = watch.what(),
                rows = arrivals.len(),
                "mail wait answered"
            );
            for message in &arrivals {
                line(&bus::message_line(message));
                append_ack(dir, None, message)?;
            }
            line("re-arm: boop wait --me &");
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            let timed_out = mailwait::timeout_line(&watch, timeout_secs, &command);
            info!(
                watching = watch.what(),
                timeout_secs,
                exit_code = 124,
                "mail wait timed out"
            );
            line(&timed_out);
            eprintln!("{timed_out}"); // @eprintln-ok: the re-run line must survive a redirected stdout
            std::process::exit(124);
        }
        std::thread::sleep(WAIT_POLL);
    }
}

// ---------------------------------------------------------------------------
// sweep
// ---------------------------------------------------------------------------

fn run_sweep(
    mail_dir_arg: Option<&Path>,
    box_name: Option<&str>,
    agent: Option<&str>,
    close_routeless: bool,
    max_age_days: u64,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let messages = all_messages(&dir)?;
    let pending = bus::unacked(&messages);
    if pending.is_empty() {
        println!("nothing unacked");
        return Ok(());
    }
    let cutoff_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
        .saturating_sub(max_age_days * 86_400_000);
    let mut acked = 0usize;
    let mut expired = 0usize;
    for message in &pending {
        if let Some(agent_id) = agent {
            if message.to != agent_id {
                continue;
            }
        }
        if parse_iso_ms(&message.from_timestamp).unwrap_or(0) < cutoff_ms {
            append_ack(&dir, box_name, message)?;
            expired += 1;
            println!("expired {}", message.id);
            continue;
        }
        let Some(route) = routes.get(&message.to) else {
            if close_routeless {
                append_ack(&dir, box_name, message)?;
                expired += 1;
                println!(
                    "expired {} -> {}: no registry route",
                    message.id, message.to
                );
            } else {
                println!(
                    "{} -> {}: no registry route, cannot scope the cass query (--close-routeless expires these)",
                    message.id,
                    message.to
                );
            }
            continue;
        };
        if cass_hit(route, &message.id).unwrap_or(false) {
            append_ack(&dir, box_name, message)?;
            acked += 1;
            println!("{} -> {}: acked", message.id, message.to);
        } else {
            println!(
                "{} -> {}: no transcript hit, still unacked",
                message.id, message.to
            );
        }
    }
    println!(
        "swept {} unacked, acked {acked}, expired {expired}",
        pending.len()
    );
    Ok(())
}

/// Ask `cass` whether the envelope id appears in the recipient's transcript.
fn cass_hit(route: &Route, message_id: &str) -> Result<bool> {
    let output = Command::new("cass")
        .args(["search", message_id, "--robot", "--limit", "20"])
        .output();
    let output = match output {
        Ok(output) if output.status.success() => output,
        _ => return Ok(false),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(_) => return Ok(false),
    };
    let hits = value
        .get("hits")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(hits.iter().any(|hit| {
        let source = hit
            .get("source_path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        scoped_to_agent(route, source)
    }))
}

fn scoped_to_agent(route: &Route, source_path: &str) -> bool {
    if source_path.is_empty() {
        return false;
    }
    if let Some(expected) = route.source_path.as_deref() {
        return source_path == expected;
    }
    route
        .session_id
        .as_deref()
        .map(|session_id| source_path.contains(session_id))
        .unwrap_or(false)
}

fn parse_iso_ms(text: &str) -> Option<u64> {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    OffsetDateTime::parse(text, &Rfc3339)
        .ok()
        .map(|parsed| parsed.unix_timestamp() as u64 * 1000 + parsed.millisecond() as u64)
}

// ---------------------------------------------------------------------------
// lane
// ---------------------------------------------------------------------------

struct LaneArgs {
    name: Option<String>,
    cwd: Option<String>,
    harness: Option<String>,
    brief: Option<PathBuf>,
    model: Option<String>,
    preset: Option<String>,
    variant: Option<String>,
    tmux: Option<String>,
    parent: Option<String>,
    branch: Option<String>,
    base_sha: Option<String>,
    socket: Option<String>,
    goal: Option<String>,
    mood: Option<String>,
    trace: Option<String>,
    no_start: bool,
    mail_dir: Option<PathBuf>,
    dry_run: bool,
    wait: bool,
    wait_timeout: u64,
    reclaim: bool,
}

/// Falls back to a `*coordinator*` name match only when no route declares
/// `kind == "coordinator"`, so a pre-`kind` registry row still resolves.
fn resolve_parent_with_legacy_fallback(
    explicit: Option<&str>,
    caller_lane: Option<&str>,
    routes: &BTreeMap<String, Route>,
) -> lane::ParentPick {
    let picked = lane::resolve_parent(explicit, caller_lane, routes);
    if picked.parent.is_some() || routes.values().any(|route| route.kind == "coordinator") {
        return picked;
    }
    let mut legacy = routes.keys().filter(|name| name.contains("coordinator"));
    match (legacy.next(), legacy.next()) {
        (Some(only), None) => lane::ParentPick {
            parent: Some(only.clone()),
            source: "registry-legacy",
        },
        _ => picked,
    }
}

/// What the warm-up will do to a fresh worktree of `repo`, for `--dry-run`.
fn start_plan(repo: &Path, no_start: bool) -> Result<String> {
    let recipe = boop::worktree::find_start_recipe(repo)?;
    Ok(match (no_start, recipe) {
        (true, _) => "boop-start: skipped (--no-start)".to_owned(),
        (false, Some(recipe)) => format!("boop-start: will run from {}", recipe.justfile.display()),
        (false, None) => format!(
            "boop-start: no recipe in {}, nothing to warm",
            repo.display()
        ),
    })
}

/// Register and spawn a lane. No match on harness id here; the adapter's own
/// `spawn`/`preview_command` decides how `prompt` becomes a real invocation.
fn run_lane(registry: &Registry, args: LaneArgs) -> Result<()> {
    let config_path = config::default_path()?;
    let config = config::load(&config_path)?;
    let model_given = args.model.is_some();
    let requested_model = match (args.model, args.preset.as_deref()) {
        (Some(model), _) => Some(model),
        (None, Some(preset)) => Some(config::resolve_model(preset, &config_path)?),
        (None, None) => None,
    };
    let harness_id = lane::harness_for_spawn(args.harness.as_deref(), requested_model.as_deref())?;
    let adapter = harness_by_id(registry, &harness_id)?;
    let repo = match &args.cwd {
        Some(cwd) => PathBuf::from(cwd),
        None => lane::repo_root(&std::env::current_dir().context("read the current directory")?)?,
    };
    let identity = lane::derive(
        &repo,
        args.branch.as_deref(),
        args.name.as_deref(),
        args.tmux.as_deref(),
    )?;
    // The binary's own sha rides the first line of every spawn: a lane that
    // dies is otherwise impossible to tie to the boop that spawned it.
    info!(
        lane = identity.lane,
        tmux_target = identity.tmux,
        harness = harness_id,
        cwd = %repo.display(),
        boop_build = boop::BUILD,
        "lane create resolved"
    );
    let worktree_mode = identity.worktree_dir.is_some();
    let brief = args.brief.clone().unwrap_or_else(|| repo.join("brief.md"));
    if !brief.is_absolute() {
        anyhow::bail!("brief path must be absolute: {}", brief.display());
    }
    if !brief.exists() {
        anyhow::bail!("brief path does not exist: {}", brief.display());
    }
    // A mood name is checked before anything spawns: a typo must not reach a
    // pane that then mails its coordinator in the default shape.
    if let Some(mood) = args.mood.as_deref() {
        boop::Store::open(boop::Store::default_path()?)?.check_mood_name(mood)?;
    }
    let default_preset = default_preset_for_harness(&config, &config_path, &harness_id)?;
    let model = config::resolve_spawn_model(
        requested_model.as_deref(),
        None,
        default_preset.as_deref(),
        &config_path,
    )?;
    // The preset that resolved the model also carries the variant; an explicit
    // --model opts out of both preset lookups. CLI --variant wins over preset.
    let preset_name = if model_given {
        None
    } else {
        args.preset.as_deref().or(default_preset.as_deref())
    };
    let variant = match args.variant {
        Some(variant) => Some(variant),
        None => preset_name
            .and_then(|name| config::resolve_variant(name, &config_path).ok())
            .flatten(),
    };
    if variant.is_some() && harness_id == "codex" {
        anyhow::bail!(
            "--variant is opencode-only; the codex channel sets reasoning effort via the \
             `model@effort` suffix instead"
        );
    }
    let prompt = brief.display().to_string();
    // A worktree branches from origin/main unless pinned; the repo-tree shape
    // keeps its own HEAD, where a base of origin/main would be a merge.
    let base = match (&args.base_sha, worktree_mode) {
        (Some(sha), _) => lane::BaseSha {
            sha: sha.clone(),
            rev: "--base-sha".to_owned(),
        },
        (None, true) => lane::default_base_sha(&repo)?,
        (None, false) => lane::BaseSha {
            sha: git_head(&repo.display().to_string())?.unwrap_or_else(|| "HEAD".into()),
            rev: "HEAD".to_owned(),
        },
    };
    let hail_mail_dir = mail_dir(args.mail_dir.as_deref())?;
    let mut routes = bus::read_routes(&hail_mail_dir)?;
    let caller = identity::resolve(&routes)?;
    register_fresh_codex_spawner(&hail_mail_dir, &repo, &caller, &mut routes)?;
    let caller_lane = caller.lane.clone().filter(|lane| *lane != identity.lane);
    let parent = resolve_parent_with_legacy_fallback(
        args.parent.as_deref(),
        caller_lane.as_deref(),
        &routes,
    );
    // A parentless foreground waiter owns a private route parent. The
    // supervisor remains the sole completion-row writer, including when the
    // pane is killed before its route-only epilogue runs.
    let result_recipient =
        completion_recipient(parent.parent.as_deref(), args.wait, &identity.lane);
    let on_exit = result_recipient
        .as_ref()
        .map(|_| lane::pane_epilogue(&identity.lane, &hail_mail_dir));

    if args.dry_run {
        info!(
            lane = identity.lane,
            harness = harness_id,
            "lane create dry run"
        );
        let spec = boop::harness::SpawnSpec {
            harness: harness_id.clone(),
            branch: identity.branch.clone(),
            base_sha: base.sha.clone(),
            main_tree: !worktree_mode,
            setup: Vec::new(),
            prompt: prompt.clone(),
            resume_session: None,
            socket: args.socket.clone(),
            worktree_dir: identity.worktree_dir.clone(),
            repo: repo.clone(),
            env_stamp: Some(spawn_env_stamp(
                &identity.lane,
                &harness_id,
                parent.parent.as_deref(),
            )),
            model: model.clone(),
            variant: variant.clone(),
            on_exit: on_exit.clone(),
            tmux: Some(identity.tmux.clone()),
            lane: identity.lane.clone(),
            mail_dir: hail_mail_dir.clone(),
            warm_start: !args.no_start,
        };
        let command = adapter
            .preview_command(&spec)
            .unwrap_or_else(|| format!("{} {}", adapter.id(), shell_quote(&prompt)));
        println!("cmd: {command}");
        println!("to: {}", identity.lane);
        println!("cwd: {}", repo.display());
        println!("harness: {harness_id}");
        match lane::kind_of(&identity.branch) {
            Some(kind) => println!("branch: {} (kind {kind})", identity.branch),
            None => println!("branch: {}", identity.branch),
        }
        if let Some(worktree_dir) = &identity.worktree_dir {
            println!("worktree: {}", worktree_dir.display());
        }
        println!("{}", start_plan(&repo, args.no_start)?);
        println!("base-sha: {} (from {})", base.sha, base.rev);
        println!("tmux: {}", identity.tmux);
        match &parent.parent {
            Some(name) => println!(
                "parent: {name} (from {}; completion hail appended on exit)",
                parent.source
            ),
            None if args.wait => {
                println!("parent: - (foreground wait owns the completion receipt)")
            }
            None => println!("parent: - (no completion hail; pass --parent <lane>)"),
        }
        if let Some(goal) = &args.goal {
            println!("goal: {goal}");
        }
        if let Some(mood) = &args.mood {
            println!("mood: {mood}");
        }
        if args.wait {
            println!(
                "wait: for {} result, timeout {}s",
                identity.lane, args.wait_timeout
            );
        }
        if args.reclaim {
            println!("reclaim: worktree and branch removed first, if the name is dead");
        }
        return Ok(());
    }
    if args.reclaim {
        let removed = lane::reclaim_for_spawn(&repo, &identity, &routes, |target| {
            tmux::mux().target_alive(None, target)
        })?;
        for line in removed.lines() {
            println!("reclaim: {line}");
        }
    }
    let lane_id = identity.lane.clone();
    let trace = args
        .trace
        .clone()
        .unwrap_or_else(|| format!("trace-{}", identity.lane));
    record_lane_purpose(
        &identity.lane,
        &trace,
        &harness_id,
        &identity.branch,
        &repo,
        model.as_deref(),
        parent.parent.as_deref(),
        args.goal.as_deref(),
        &brief,
    );
    if let Some(mood) = args.mood.as_deref() {
        record_lane_mood(&identity.lane, mood)?;
    }
    run_dispatch(
        registry,
        DispatchArgs {
            to: identity.lane,
            cwd: repo.display().to_string(),
            cmd: prompt,
            from: None,
            harness: Some(harness_id.clone()),
            session_id: None,
            model,
            mode: Some("auto".into()),
            tmux: Some(identity.tmux),
            socket: args.socket,
            body: Some(format!(
                "Read and execute the lane brief at {}",
                brief.display()
            )),
            r#ref: Some(brief.display().to_string()),
            mail_dir: args.mail_dir,
            resolve_wait: 3,
            main_tree: !worktree_mode,
            base_sha: Some(base.sha),
            branch: Some(identity.branch),
            worktree_dir: identity.worktree_dir,
            parent: result_recipient,
            goal: args.goal.clone(),
            on_exit,
            warm_start: !args.no_start,
            variant: variant.clone(),
        },
    )?;
    info!(
        lane = lane_id,
        harness = harness_id,
        "lane create dispatched"
    );
    if args.wait {
        // Same code path as `beep lane wait`, which exits with the lane's rc.
        return run_lane_wait(Some(&hail_mail_dir), &lane_id, args.wait_timeout);
    }
    Ok(())
}

/// A Codex tool process carries an exact thread and pane before Boop has seen
/// it. Persist that observed caller before selecting the child's parent so the
/// completion hail has a pane-backed coordinator route on the first spawn.
fn register_fresh_codex_spawner(
    mail_dir: &Path,
    cwd: &Path,
    caller: &identity::Identity,
    routes: &mut BTreeMap<String, Route>,
) -> Result<()> {
    if caller.rung != Some(identity::Rung::CodexProcess) {
        return Ok(());
    }
    let lane = caller.lane.as_deref().context("Codex caller lane")?;
    if routes.contains_key(lane) {
        return Ok(());
    }
    let route = Route {
        kind: "coordinator".to_owned(),
        harness: Some("codex".to_owned()),
        tmux: caller.pane.clone(),
        cwd: Some(cwd.display().to_string()),
        model: None,
        mode: Some("interactive".to_owned()),
        session_id: caller.session.clone(),
        source_path: None,
        parent: None,
        goal: None,
        registered_at: Some(bus::now_iso()),
        base_sha: None,
        worktree_dir: None,
    };
    write_route(mail_dir, lane, route.clone())?;
    routes.insert(lane.to_owned(), route);
    Ok(())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

fn completion_recipient(parent: Option<&str>, wait: bool, lane: &str) -> Option<String> {
    parent
        .map(str::to_owned)
        .or_else(|| wait.then(|| format!("__wait__{lane}")))
}

// ---------------------------------------------------------------------------
// adopt / prune + bus store helpers
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
/// What an adopt does about the adopted session's hook inbox.
struct HookWiring {
    no_hooks: bool,
    uninstall: bool,
}

#[allow(clippy::too_many_arguments)]
fn run_adopt(
    name: &str,
    kind: &str,
    tmux_session: &str,
    harness: Option<&str>,
    session_id: Option<&str>,
    cwd: Option<&str>,
    model: Option<&str>,
    mode: Option<&str>,
    parent: Option<&str>,
    goal: Option<&str>,
    mail_dir_arg: Option<&Path>,
    hooks: HookWiring,
) -> Result<()> {
    let registry = Registry::discover();
    let processes = crate::proc::SysinfoSnapshot::capture()?;
    run_adopt_with(
        name,
        kind,
        tmux_session,
        harness,
        session_id,
        cwd,
        model,
        mode,
        parent,
        goal,
        mail_dir_arg,
        hooks,
        &registry,
        tmux::mux(),
        &processes,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_adopt_with(
    name: &str,
    kind: &str,
    tmux_session: &str,
    harness: Option<&str>,
    session_id: Option<&str>,
    cwd: Option<&str>,
    model: Option<&str>,
    mode: Option<&str>,
    parent: Option<&str>,
    goal: Option<&str>,
    mail_dir_arg: Option<&Path>,
    hooks: HookWiring,
    registry: &Registry,
    multiplexer: &dyn tmux::Multiplexer,
    processes: &dyn crate::proc::ProcReader,
) -> Result<()> {
    // Taking the hooks out is about a project directory, not about a pane, and
    // the pane is usually already gone by the time anyone wants that.
    if hooks.uninstall {
        let project = adopt_cwd(cwd)?;
        let changed = write_inbox_hooks(&project, name, true)?;
        report_inbox_hooks(&project, name, true, changed);
        return Ok(());
    }
    if !multiplexer.has_session(None, tmux_session)? {
        println!("refusing adopt {name}: no such tmux session {tmux_session}");
        return Ok(());
    }
    let dir = mail_dir(mail_dir_arg)?;
    let existing = bus::read_routes(&dir)?.remove(name);
    let discovered_session = session_id.map(str::to_owned).or_else(|| {
        harness.and_then(|id| {
            registry.by_id(id).and_then(|adapter| {
                adapter.session_id_in_pane(multiplexer, processes, tmux_session)
            })
        })
    });
    let route = Route {
        kind: kind.into(),
        harness: harness.map(str::to_owned),
        tmux: Some(tmux_session.to_owned()),
        cwd: cwd.map(str::to_owned),
        model: model.map(str::to_owned),
        mode: mode.map(str::to_owned),
        session_id: discovered_session.or_else(|| existing.and_then(|route| route.session_id)),
        source_path: None,
        parent: parent.map(str::to_owned),
        goal: goal.map(str::to_owned),
        registered_at: Some(bus::now_iso()),
        base_sha: None,
        worktree_dir: None,
    };
    write_route(&dir, name, route)?;
    println!("adopted {name} -> tmux {tmux_session}");
    // A claude pane is driven by a model between turns, so mail belongs at a
    // turn boundary; every other harness keeps pane injection.
    let claude = harness == Some("claude");
    if claude && !hooks.no_hooks {
        let project = adopt_cwd(cwd)?;
        let changed = write_inbox_hooks(&project, name, false)?;
        report_inbox_hooks(&project, name, false, changed);
        println!("hails to {name} now queue for the hook inbox, never its keyboard");
    }
    Ok(())
}

/// The project directory whose settings carry an adopted session's hooks.
fn adopt_cwd(cwd: Option<&str>) -> Result<PathBuf> {
    match cwd {
        Some(cwd) => Ok(PathBuf::from(cwd)),
        None => std::env::current_dir().context("read the current directory"),
    }
}

fn run_prune(mail_dir_arg: Option<&Path>) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    if tmux::mux().live_sessions(None).is_none() {
        println!("refusing prune: tmux unreachable, cannot tell live from dead");
        return Ok(());
    }
    let routes = bus::read_routes(&dir)?;
    let dead: Vec<String> = routes
        .iter()
        .filter(|(_, route)| route.kind == "lane")
        .filter(|(_, route)| {
            let Some(target) = route.tmux.as_deref() else {
                return true;
            };
            !tmux::mux().target_alive(None, target)
        })
        .map(|(name, _)| name.clone())
        .collect();
    let path = dir.join("registry.json");
    bus::cas_update_json(&path, |current| {
        for name in &dead {
            current.remove(name);
        }
        Ok(())
    })?;
    info!(routes_deleted = dead.len(), mail_dir = %dir.display(), "lane routes pruned");
    println!("pruned {} dead routes", dead.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser, Subcommand};
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;

    use super::{
        agent_session_graph_query, agent_summary_text, append_message, command_needs_startup_sync,
        completion_recipient, config, dead_reason, default_preset_for_harness, ident, lane_state,
        register_fresh_codex_spawner, resolve_dispatch_harness,
        resolve_parent_with_legacy_fallback, route_liveness, run_adopt_with, run_agent,
        run_lane_delete, run_lane_prune, run_ps_with, run_with_startup_sync, session_matches_route,
        write_line, write_route, AgentCmd, AgentSessionGraphFormat, AgentSummaryCmd, Cli, DbCmd,
        HookWiring, MeCmd, SubCmd,
    };
    use boop::bus::{self, read_routes, Route};
    use boop::proc::{ProcReader, ProcessInfo, SysinfoSnapshot};
    use boop::registry::Registry;
    use boop::tmux::{LiveSessions, Multiplexer};
    use boop::{
        AgentRuntimeRow, AgentSummary, AgentSummaryActivity, AgentSummaryAgent, MailboxCounts,
        ProcessLiveness, RuntimeLiveness, TmuxLiveness, WorktreeCoordinates,
    };

    #[test]
    fn foreground_wait_owns_a_result_recipient_without_a_parent() {
        assert_eq!(
            completion_recipient(None, true, "feature-a"),
            Some("__wait__feature-a".into())
        );
        assert_eq!(
            completion_recipient(Some("coordinator"), true, "feature-a"),
            Some("coordinator".into())
        );
        assert_eq!(completion_recipient(None, false, "feature-a"), None);
    }

    #[test]
    fn a_first_codex_spawn_registers_its_observed_pane_as_the_parent_route() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).expect("mail dir");
        let cwd = std::path::Path::new("/tmp/unrecorded-worktree");
        let caller = boop::identity::Identity {
            session: Some("thread-7".to_owned()),
            lane: Some("codex-1206".to_owned()),
            parent: None,
            harness: Some("codex".to_owned()),
            pane: Some("%1206".to_owned()),
            rung: Some(boop::identity::Rung::CodexProcess),
        };
        let mut routes = BTreeMap::new();

        register_fresh_codex_spawner(&dir, cwd, &caller, &mut routes).expect("register caller");
        let persisted = read_routes(&dir).expect("persisted routes");
        let memory = routes.get("codex-1206").expect("memory route");
        let disk = persisted.get("codex-1206").expect("disk route");

        for route in [memory, disk] {
            assert_eq!(route.kind, "coordinator");
            assert_eq!(route.harness.as_deref(), Some("codex"));
            assert_eq!(route.tmux.as_deref(), Some("%1206"));
            assert_eq!(route.cwd.as_deref(), Some("/tmp/unrecorded-worktree"));
            assert_eq!(route.mode.as_deref(), Some("interactive"));
            assert_eq!(route.session_id.as_deref(), Some("thread-7"));
        }
        std::fs::remove_dir_all(dir).expect("remove mail dir");
    }

    struct ClaudeProcessFixture;

    struct AdoptMux;

    impl Multiplexer for AdoptMux {
        fn current_pane(&self, _: Option<&str>) -> Option<String> {
            None
        }
        fn session_of_pane(&self, _: Option<&str>, _: &str) -> Option<String> {
            None
        }
        fn pane_pid(&self, _: Option<&str>, _: &str) -> Option<u32> {
            Some(10)
        }
        fn live_sessions(&self, _: Option<&str>) -> Option<LiveSessions> {
            Some(LiveSessions {
                names: ["sprefa-5".into()].into_iter().collect(),
            })
        }
        fn has_session(&self, _: Option<&str>, target: &str) -> anyhow::Result<bool> {
            Ok(target.split(':').next() == Some("sprefa-5"))
        }
        fn kill_session(&self, _: Option<&str>, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn target_alive(&self, _: Option<&str>, _: &str) -> bool {
            true
        }
        fn capture_pane(&self, _: Option<&str>, _: &str, _: Option<u32>) -> anyhow::Result<String> {
            Ok(String::new())
        }
        fn new_detached_session(
            &self,
            _: Option<&str>,
            _: &str,
            _: &str,
            _: &str,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        fn new_bare_session(&self, _: Option<&str>, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn send_keys_literal(&self, _: Option<&str>, _: &str, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn send_text(&self, _: Option<&str>, _: &str, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn send_key_named(&self, _: Option<&str>, _: &str, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn new_window(
            &self,
            _: Option<&str>,
            _: &str,
            _: &str,
            _: &str,
            _: &str,
        ) -> anyhow::Result<String> {
            Ok(String::new())
        }
        fn swap_windows(&self, _: Option<&str>, _: &str, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn kill_window(&self, _: Option<&str>, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
    }

    impl ProcReader for ClaudeProcessFixture {
        fn is_alive(&self, pid: u32) -> bool {
            pid == 10 || pid == 11
        }
        fn process(&self, pid: u32) -> Option<ProcessInfo> {
            match pid {
                10 => Some(ProcessInfo {
                    pid,
                    parent: None,
                    name: "shell".into(),
                    command: vec!["zsh".into()],
                    rss_bytes: 0,
                    cpu_percent: 0.0,
                    start_time_secs: 0,
                    cwd: None,
                }),
                11 => Some(ProcessInfo {
                    pid,
                    parent: Some(10),
                    name: "claude".into(),
                    command: vec![
                        "claude".into(),
                        "--resume".into(),
                        "da6da0ca-5ad6-4f2f-88f7-de82e79f1e6b".into(),
                    ],
                    rss_bytes: 0,
                    cpu_percent: 0.0,
                    start_time_secs: 0,
                    cwd: None,
                }),
                _ => None,
            }
        }
        fn children(&self, pid: u32) -> Vec<u32> {
            (pid == 10).then_some(11).into_iter().collect()
        }
        fn descendants(&self, pid: u32) -> Vec<u32> {
            (pid == 10).then_some(11).into_iter().collect()
        }
        fn descendant_count(&self, pid: u32) -> usize {
            usize::from(pid == 10)
        }
    }

    #[test]
    fn adopt_discovers_claude_resume_identity_and_explicit_id_wins() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let mux = AdoptMux;
        let processes = ClaudeProcessFixture;
        let registry = Registry::discover();
        let hooks = || HookWiring {
            no_hooks: true,
            uninstall: false,
        };

        run_adopt_with(
            "sprefa-coordinator",
            "coordinator",
            "sprefa-5:0.0",
            Some("claude"),
            None,
            Some("/repo"),
            None,
            None,
            None,
            None,
            Some(&dir),
            hooks(),
            &registry,
            &mux,
            &processes,
        )
        .unwrap();
        let discovered = read_routes(&dir).unwrap();
        assert_eq!(
            discovered["sprefa-coordinator"].session_id.as_deref(),
            Some("da6da0ca-5ad6-4f2f-88f7-de82e79f1e6b")
        );

        run_adopt_with(
            "sprefa-coordinator",
            "coordinator",
            "sprefa-5:0.0",
            Some("claude"),
            Some("explicit-session"),
            Some("/repo"),
            None,
            None,
            None,
            None,
            Some(&dir),
            hooks(),
            &registry,
            &mux,
            &processes,
        )
        .unwrap();
        let explicit = read_routes(&dir).unwrap();
        assert_eq!(
            explicit["sprefa-coordinator"].session_id.as_deref(),
            Some("explicit-session")
        );
        assert_eq!(explicit.len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn public_agent_summary_command_parses() {
        let cli = Cli::try_parse_from(["boop", "agent", "summary", "--format", "text"])
            .expect("public agent summary command parses");
        assert!(matches!(
            cli.command,
            SubCmd::Agent {
                cmd: AgentSummaryCmd::Summary { .. }
            }
        ));
    }

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
    fn me_favorite_defaults_to_the_newest_assistant_message() {
        let cli = Cli::try_parse_from(["boop", "me", "favorite"])
            .expect("caller-relative favorite command parses");
        assert!(matches!(
            cli.command,
            SubCmd::Me {
                cmd: Some(MeCmd::Favorite { index: -1, .. }),
                ..
            }
        ));
    }

    #[test]
    fn me_favorite_accepts_an_older_negative_position() {
        let cli = Cli::try_parse_from(["boop", "me", "favorite", "-2", "--note", "keep"])
            .expect("negative favorite position parses");
        assert!(matches!(
            cli.command,
            SubCmd::Me {
                cmd: Some(MeCmd::Favorite { index: -2, .. }),
                ..
            }
        ));
    }

    /// FAIL-PRE-FIX: `adopt`, `beep agent register|done` and `beep lane list`
    /// synced and read no `agent_*` row; the four `!` lines below then failed.
    #[test]
    fn startup_sync_policy_limits_projection_to_transcript_consumers() {
        let registry_only = [
            vec!["boop", "adopt", "--name", "root", "--tmux", "root"],
            vec![
                "boop", "hail", "--to", "root", "--from", "lane", "--body", "done",
            ],
            vec!["boop", "inbox", "drain", "--as", "root"],
            vec!["boop", "beep", "agent", "register", "worker"],
            vec!["boop", "beep", "agent", "done", "worker"],
            vec!["boop", "beep", "lane", "list"],
            vec!["boop", "beep", "lane", "create", "--branch", "fix/x"],
        ];
        for argv in registry_only {
            let cli = Cli::try_parse_from(&argv).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
            assert!(
                !command_needs_startup_sync(&cli.command),
                "{argv:?} reads no agent_* row and must not sync"
            );
        }
        let transcript_readers = [
            vec!["boop", "agent", "summary"],
            vec!["boop", "db", "turn", "list"],
            vec!["boop", "db", "status"],
        ];
        for argv in transcript_readers {
            let cli = Cli::try_parse_from(&argv).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
            assert!(
                command_needs_startup_sync(&cli.command),
                "{argv:?} reads agent_* rows and must sync first"
            );
        }
    }

    #[test]
    fn startup_sync_runs_once_before_the_command() {
        let calls = std::cell::RefCell::new(Vec::new());
        run_with_startup_sync(
            true,
            || {
                calls.borrow_mut().push("sync");
                Ok(())
            },
            || {
                calls.borrow_mut().push("run");
                Ok(())
            },
        )
        .expect("startup sync orchestration");
        assert_eq!(*calls.borrow(), ["sync", "run"]);
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

    /// A named harness that is not registered must be refused, never quietly
    /// swapped for the first adapter, which would be a capability lie.
    #[test]
    fn dispatch_refuses_an_unregistered_harness() {
        let registry = Registry::discover();
        let error = match resolve_dispatch_harness(&registry, Some("gemini-cli")) {
            Ok(_) => panic!("unregistered harness must be refused"),
            Err(error) => error,
        };
        let message = error.to_string();
        assert!(message.contains("gemini-cli"), "message: {message}");
        assert!(message.contains("claude"), "registered set: {message}");
        assert!(message.contains("opencode"), "registered set: {message}");
    }

    /// Sabotage receipt: dropping the harness-fit guard makes this assert the
    /// codex arm, spelling `codex exec -m openrouter/...`, which cannot run.
    #[test]
    fn the_default_preset_reaches_only_its_own_harness() {
        let dir = std::env::temp_dir().join("boop-default-preset-fit");
        std::fs::create_dir_all(&dir).expect("create the probe directory");
        let path = dir.join("config.json");
        std::fs::write(
            &path,
            r#"{ "default-model-preset": "flash4",
                 "model-presets": { "flash4": "openrouter/deepseek/deepseek-v4-flash-0731" } }"#,
        )
        .expect("write the probe config");
        let config = config::load(&path).expect("load the probe config");
        assert_eq!(
            default_preset_for_harness(&config, &path, "opencode").unwrap(),
            Some("flash4".to_owned())
        );
        assert_eq!(
            default_preset_for_harness(&config, &path, "codex").unwrap(),
            None
        );
    }

    fn temp_mail_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        std::env::temp_dir().join(format!(
            "boop_mail_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn native_registration_stays_live_until_explicit_done_and_done_is_once() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        run_agent(AgentCmd::Register {
            name: "native-child".into(),
            kind: "native".into(),
            parent: Some("coordinator".into()),
            on_parent_death: crate::ParentDeathPolicy::Orphan,
            worktree: None,
            mail_dir: Some(dir.clone()),
        })
        .unwrap();

        let route = read_routes(&dir).unwrap().remove("native-child").unwrap();
        assert_eq!(
            lane_state(&Some(boop::tmux::LiveSessions::default()), &route),
            "live"
        );
        assert_eq!(
            route_liveness(&dir, "native-child"),
            super::RouteLiveness::Live
        );

        run_agent(AgentCmd::Done {
            name: "native-child".into(),
            rc: 7,
            mail_dir: Some(dir.clone()),
        })
        .unwrap();
        assert!(!read_routes(&dir).unwrap().contains_key("native-child"));

        let second = run_agent(AgentCmd::Done {
            name: "native-child".into(),
            rc: 7,
            mail_dir: Some(dir.clone()),
        });
        assert!(
            second.is_err(),
            "a completed native route cannot complete twice"
        );
        let messages = bus::read_boxes(&dir)
            .unwrap()
            .into_iter()
            .flat_map(|path| bus::parse_box(&path))
            .filter(|message| message.kind == "result" && message.from == "native-child")
            .collect::<Vec<_>>();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].to, "coordinator");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT (Job 3b). A `--route-only` delete drops the lane's registry row
    /// without touching pane or tmux, so the on-exit epilogue cleans up in-pane.
    #[test]
    fn route_only_delete_drops_the_registry_row_without_tmux() {
        let dir = temp_mail_dir();
        write_route(
            &dir,
            "l",
            Route {
                kind: "lane".into(),
                harness: Some("claude".into()),
                tmux: Some("somesession".into()),
                cwd: None,
                model: None,
                mode: None,
                session_id: None,
                source_path: None,
                parent: None,
                goal: None,
                registered_at: None,
                base_sha: None,
                worktree_dir: None,
            },
        )
        .unwrap();
        run_lane_delete(Some(&dir), "l", true).unwrap();
        let routes = read_routes(&dir).unwrap();
        assert!(
            !routes.contains_key("l"),
            "a finished lane must leave no registry row"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn unique_name(prefix: &str) -> String {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn tmux_route(tmux_name: &str) -> Route {
        Route {
            kind: "lane".into(),
            harness: Some("claude".into()),
            tmux: Some(tmux_name.to_owned()),
            cwd: None,
            model: None,
            mode: None,
            session_id: None,
            source_path: None,
            parent: None,
            goal: None,
            registered_at: None,
            base_sha: None,
            worktree_dir: None,
        }
    }

    /// A real session on the default tmux server, killed on drop; `lane
    /// prune` hardcodes the default server, so a "live" fixture needs one.
    struct LiveTmuxSession {
        name: String,
    }

    impl LiveTmuxSession {
        fn new(name: &str) -> Self {
            crate::tmux::mux()
                .new_bare_session(None, name)
                .expect("tmux installed and reachable");
            LiveTmuxSession {
                name: name.to_owned(),
            }
        }
    }

    impl Drop for LiveTmuxSession {
        fn drop(&mut self) {
            let _ = crate::tmux::mux().kill_session(None, &self.name);
        }
    }

    /// FAIL-FIRST. Before `run_lane_prune` existed this had no callee to
    /// assert against; now: a dead row is gone, a live row survives.
    #[test]
    fn prune_removes_a_dead_row_and_keeps_a_live_one() {
        let dir = temp_mail_dir();
        let live_name = unique_name("boop-prune-live");
        let _session = LiveTmuxSession::new(&live_name);
        write_route(
            &dir,
            "dead-lane",
            tmux_route(&unique_name("boop-prune-dead")),
        )
        .unwrap();
        write_route(&dir, "live-lane", tmux_route(&live_name)).unwrap();

        run_lane_prune(Some(&dir), false).unwrap();

        let routes = read_routes(&dir).unwrap();
        assert!(
            !routes.contains_key("dead-lane"),
            "a dead row must be pruned"
        );
        assert!(
            routes.contains_key("live-lane"),
            "a live row must survive prune"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. `--dry-run` reports the same rows a real run would prune but
    /// removes nothing.
    #[test]
    fn prune_dry_run_removes_nothing() {
        let dir = temp_mail_dir();
        write_route(
            &dir,
            "dead-lane",
            tmux_route(&unique_name("boop-prune-dead")),
        )
        .unwrap();

        run_lane_prune(Some(&dir), true).unwrap();

        let routes = read_routes(&dir).unwrap();
        assert!(
            routes.contains_key("dead-lane"),
            "--dry-run must remove nothing"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dead_reason_names_a_gone_session_with_no_pid() {
        let snapshot = SysinfoSnapshot::capture().unwrap();
        let route = tmux_route(&unique_name("boop-prune-nonexistent"));
        assert_eq!(
            dead_reason(&route, &snapshot).as_deref(),
            Some("tmux session gone, no pid recorded")
        );
    }

    #[test]
    fn dead_reason_is_none_for_a_live_session() {
        let name = unique_name("boop-prune-alive");
        let _session = LiveTmuxSession::new(&name);
        let snapshot = SysinfoSnapshot::capture().unwrap();
        assert_eq!(dead_reason(&tmux_route(&name), &snapshot), None);
    }

    #[test]
    fn dead_reason_names_no_recorded_session_when_tmux_is_absent() {
        let snapshot = SysinfoSnapshot::capture().unwrap();
        let route = Route {
            kind: "lane".into(),
            harness: Some("claude".into()),
            tmux: None,
            cwd: None,
            model: None,
            mode: None,
            session_id: None,
            source_path: None,
            parent: None,
            goal: None,
            registered_at: None,
            base_sha: None,
            worktree_dir: None,
        };
        assert_eq!(
            dead_reason(&route, &snapshot).as_deref(),
            Some("no tmux session recorded")
        );
    }

    fn result_message(id: &str, lane: &str, rc: i32) -> boop::bus::Message {
        boop::bus::Message {
            id: id.into(),
            from: lane.into(),
            to: lane.into(),
            from_timestamp: "2026-08-01T00:00:00.000Z".into(),
            to_timestamp: None,
            kind: "result".into(),
            reply_to: None,
            body: format!("lane {lane} done rc={rc}"),
            r#ref: None,
            rc: Some(rc),
            detail: None,
        }
    }

    fn registered_route(ts: &str) -> Route {
        Route {
            kind: "lane".into(),
            harness: Some("claude".into()),
            tmux: Some("l".into()),
            cwd: None,
            model: None,
            mode: None,
            session_id: None,
            source_path: None,
            parent: None,
            goal: None,
            registered_at: Some(ts.into()),
            base_sha: None,
            worktree_dir: None,
        }
    }

    /// RECEIPT (was wait_returns_rc_from_a_preexisting_result_row; pre-fix
    /// rc=0). Older than the spawn's registration: skipped, times out (124).
    #[test]
    fn wait_skips_a_result_row_older_than_the_current_spawn() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        append_message(&dir, &result_message("m-1", "l", 5)).unwrap();
        write_route(&dir, "l", registered_route("2026-08-02T00:00:00.000Z")).unwrap();
        assert_eq!(
            super::wait_for_result(
                &dir,
                "l",
                Some(std::time::Duration::from_millis(60)),
                std::time::Duration::from_millis(10),
            ),
            None,
            "an older row is skipped, so the wait times out (exits 124)"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A result row at or after the current spawn's registration satisfies the
    /// wait immediately with the rc its body names.
    #[test]
    fn wait_accepts_a_result_row_after_the_current_spawn() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        write_route(&dir, "l", registered_route("2026-08-01T00:00:00.000Z")).unwrap();
        let mut message = result_message("m-2", "l", 7);
        message.from_timestamp = "2026-08-02T00:00:00.000Z".into();
        append_message(&dir, &message).unwrap();
        assert_eq!(
            super::wait_for_result(
                &dir,
                "l",
                Some(std::time::Duration::from_secs(2)),
                std::time::Duration::from_millis(10),
            ),
            Some(7)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT (contract 3). No route row survives, so any result row
    /// satisfies: the after-the-fact read.
    #[test]
    fn wait_accepts_a_result_row_with_no_route_registered() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let mut message = result_message("m-3", "l", 4);
        message.from_timestamp = "2026-07-01T00:00:00.000Z".into();
        append_message(&dir, &message).unwrap();
        assert_eq!(
            super::wait_for_result(
                &dir,
                "l",
                Some(std::time::Duration::from_secs(2)),
                std::time::Duration::from_millis(10),
            ),
            Some(4)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT (Job 3). An empty mailbox times out to `None`, which the verb
    /// maps to exit code 124.
    #[test]
    fn wait_times_out_when_no_result_row_arrives() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let outcome = super::wait_for_result(
            &dir,
            "l",
            Some(std::time::Duration::from_millis(60)),
            std::time::Duration::from_millis(10),
        );
        assert_eq!(
            outcome, None,
            "timeout returns the None the verb exits 124 on"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A non-result row for the lane never satisfies the wait.
    #[test]
    fn a_non_result_row_does_not_satisfy_the_wait() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let mut message = result_message("m-2", "l", 3);
        message.kind = "note".into();
        append_message(&dir, &message).unwrap();
        assert_eq!(super::lane_result_rc(&dir, "l"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. The completion row is hailed `--to <parent> --from <lane>`, so a
    /// wait keyed on the recipient never saw the row it exists to wait for.
    #[test]
    fn wait_matches_the_row_the_supervisor_actually_writes() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let mut message = result_message("m-3", "feature-schema-emit", 0);
        message.to = "sprefa-coordinator".into();
        append_message(&dir, &message).unwrap();
        assert_eq!(super::lane_result_rc(&dir, "feature-schema-emit"), Some(0));
        assert_eq!(
            super::wait_for_result(
                &dir,
                "feature-schema-emit",
                Some(std::time::Duration::from_secs(2)),
                std::time::Duration::from_millis(10),
            ),
            Some(0)
        );
        assert_eq!(
            super::lane_result_rc(&dir, "some-other-lane"),
            None,
            "another lane's completion never satisfies this wait"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. A lane that fails hands its rc back through the same row, and
    /// an absent row is the 124 timeout `--wait-timeout` exits on.
    #[test]
    fn wait_propagates_a_failing_rc_and_times_out_otherwise() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            super::wait_for_result(
                &dir,
                "feature-schema-emit",
                Some(std::time::Duration::from_millis(40)),
                std::time::Duration::from_millis(10),
            ),
            None,
            "no result row yet: the verb exits 124 on this None"
        );
        let mut message = result_message("m-4", "feature-schema-emit", 17);
        message.to = "sprefa-coordinator".into();
        append_message(&dir, &message).unwrap();
        assert_eq!(super::lane_result_rc(&dir, "feature-schema-emit"), Some(17));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// FAIL-PRE-FIX: a lane whose pane evaporated left `lane wait` polling a
    /// mailbox nothing would write to, forever under `--timeout 0`.
    #[test]
    fn wait_calls_a_lane_dead_when_its_route_stops_being_live() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        write_route(&dir, "l", registered_route("2026-08-01T00:00:00.000Z")).unwrap();
        assert_eq!(
            super::wait_for_outcome(
                &dir,
                "l",
                None,
                std::time::Duration::from_millis(1),
                &|_, _| super::RouteLiveness::Dead,
            ),
            super::WaitOutcome::Died,
            "a dead route with no result row exits 3, never blocks"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Only the supervisor writes the row now, and a mailbox holding a pair
    /// from an older build still answers one rc.
    #[test]
    fn a_duplicate_result_row_leaves_the_wait_unchanged() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let mut supervisor = result_message("m-supervisor", "l", 2);
        supervisor.to = "sprefa-coordinator".into();
        append_message(&dir, &supervisor).unwrap();
        assert_eq!(super::lane_result_rc(&dir, "l"), Some(2));
        let mut older_build = result_message("m-epilogue", "l", 2);
        older_build.to = "sprefa-coordinator".into();
        append_message(&dir, &older_build).unwrap();
        assert_eq!(super::lane_result_rc(&dir, "l"), Some(2));
        assert_eq!(
            super::wait_for_outcome(
                &dir,
                "l",
                Some(std::time::Duration::from_secs(2)),
                std::time::Duration::from_millis(1),
                &|_, _| super::RouteLiveness::Unknown,
            ),
            super::WaitOutcome::Result(2)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A lane that reported is unaffected by the liveness check: its pane is
    /// already gone by the time its row is read.
    #[test]
    fn a_result_row_beats_a_dead_route() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let mut message = result_message("m-5", "l", 9);
        message.to = "sprefa-coordinator".into();
        append_message(&dir, &message).unwrap();
        assert_eq!(
            super::wait_for_outcome(
                &dir,
                "l",
                Some(std::time::Duration::from_secs(2)),
                std::time::Duration::from_millis(1),
                &|_, _| super::RouteLiveness::Dead,
            ),
            super::WaitOutcome::Result(9)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An unreachable tmux, a lane with no route and a live lane all read the
    /// same to the wait: keep polling until the deadline.
    #[test]
    fn an_undecidable_route_still_times_out_rather_than_reporting_death() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        for liveness in [super::RouteLiveness::Unknown, super::RouteLiveness::Live] {
            assert_eq!(
                super::wait_for_outcome(
                    &dir,
                    "l",
                    Some(std::time::Duration::from_millis(40)),
                    std::time::Duration::from_millis(10),
                    &|_, _| liveness,
                ),
                super::WaitOutcome::TimedOut,
                "{liveness:?} is not evidence of death"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A route reads dead for the poll or two between registration and the
    /// session answering, which must not end the wait.
    #[test]
    fn a_single_dead_observation_does_not_end_the_wait() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let polls = std::sync::atomic::AtomicU32::new(0);
        assert_eq!(
            super::wait_for_outcome(
                &dir,
                "l",
                Some(std::time::Duration::from_millis(60)),
                std::time::Duration::from_millis(10),
                &|_, _| match polls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) {
                    0 => super::RouteLiveness::Dead,
                    _ => super::RouteLiveness::Live,
                },
            ),
            super::WaitOutcome::TimedOut
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. A registry row written before the branch-derived names (lane id
    /// with no kind, `lane/*` worktree cwd) still reads, resolves and deletes.
    #[test]
    fn a_pre_branch_registry_row_still_reads_and_deletes() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("registry.json"),
            r#"{
  "boop-sql": {
    "harness": "opencode",
    "tmux": "boop-sql",
    "cwd": "/Users/x/projects/sprefa/.boop-worktrees/lane/boop-sql",
    "model": "openrouter/deepseek/deepseek-v4-flash-0731",
    "mode": "auto",
    "sessionId": "ses_0167"
  },
  "sprefa-coordinator": { "harness": "claude", "tmux": "shell:0.0" }
}"#,
        )
        .unwrap();
        let routes = read_routes(&dir).unwrap();
        let old = &routes["boop-sql"];
        assert_eq!(old.session_id.as_deref(), Some("ses_0167"));
        assert_eq!(old.tmux.as_deref(), Some("boop-sql"));
        assert!(old
            .cwd
            .as_deref()
            .unwrap()
            .contains(".boop-worktrees/lane/"));
        assert_eq!(
            resolve_parent_with_legacy_fallback(None, None, &routes)
                .parent
                .as_deref(),
            Some("sprefa-coordinator"),
            "an old row is still a usable parent default"
        );
        run_lane_delete(Some(&dir), "boop-sql", true).unwrap();
        let after = read_routes(&dir).unwrap();
        assert!(!after.contains_key("boop-sql"));
        assert!(
            after.contains_key("sprefa-coordinator"),
            "deleting one old row leaves the others"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT (boop-coordinator-by-kind compat): a pane-less coordinator is
    /// not inferred, and the legacy fallback cannot replace that decision.
    #[test]
    fn legacy_fallback_never_overrides_an_explicit_coordinator_kind() {
        let mut routes = BTreeMap::new();
        let mut boss = route_with(None);
        boss.kind = "coordinator".into();
        boss.tmux = None;
        routes.insert("boss".into(), boss);
        let pick = resolve_parent_with_legacy_fallback(None, None, &routes);
        assert_eq!(pick.parent, None);
        assert_eq!(pick.source, "none");
    }

    /// RECEIPT (job 1). A route written with --goal round-trips through the
    /// registry.
    #[test]
    fn route_goal_round_trips() {
        let dir = temp_mail_dir();
        let route = Route {
            kind: "lane".into(),
            harness: Some("opencode".into()),
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

    fn route_with(parent: Option<&str>) -> Route {
        Route {
            kind: "lane".into(),
            harness: Some("opencode".into()),
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
        }
    }

    fn dispatch(from: &str, to: &str) -> boop::bus::Message {
        boop::bus::Message {
            id: format!("m-{from}-{to}"),
            from: from.into(),
            to: to.into(),
            from_timestamp: "2026-01-01T00:00:00.000Z".into(),
            to_timestamp: None,
            kind: "dispatch".into(),
            reply_to: None,
            body: "".into(),
            r#ref: None,
            rc: None,
            detail: None,
        }
    }

    fn live_meta(pid: u32) -> super::LaneMeta {
        super::LaneMeta {
            pid,
            state: "live",
            descendants: vec![],
        }
    }

    /// RECEIPT (pstree). A route's explicit `--parent` wins over a mailbox
    /// dispatch edge that names a different summoner.
    #[test]
    fn explicit_parent_beats_inferred_dispatch() {
        let mut routes = BTreeMap::new();
        routes.insert("child".into(), route_with(Some("explicit")));
        let messages = vec![dispatch("mailbox", "child")];
        let edges = super::resolve_edges(&routes, &messages);
        let edge = &edges["child"];
        assert_eq!(edge.parent.as_deref(), Some("explicit"));
        assert!(!edge.inferred);
    }

    /// RECEIPT (pstree). An orphaned route infers its summoner from the FIRST
    /// dispatch row addressed to it, later rows ignored.
    #[test]
    fn orphan_infers_summoner_from_first_dispatch() {
        let mut routes = BTreeMap::new();
        routes.insert("child".into(), route_with(None));
        let messages = vec![
            dispatch("summoner1", "child"),
            dispatch("summoner2", "child"),
        ];
        let edges = super::resolve_edges(&routes, &messages);
        let edge = &edges["child"];
        assert_eq!(edge.parent.as_deref(), Some("summoner1"));
        assert!(edge.inferred);
    }

    /// RECEIPT (pstree). A summoner absent from the registry renders as a
    /// `[gone]` root with the orphan lane hung beneath it.
    #[test]
    fn orphan_root_prints_gone_summoner() {
        let mut routes = BTreeMap::new();
        routes.insert("child".into(), route_with(None));
        let messages = vec![dispatch("coordinator", "child")];
        let edges = super::resolve_edges(&routes, &messages);
        let mut meta = BTreeMap::new();
        meta.insert("child".into(), live_meta(4242));
        let mut include = BTreeSet::new();
        include.insert("child".into());
        let nodes = super::build_lane_nodes(&routes, &edges, &meta, &include);
        let text = super::render_text(&nodes);
        let joined = text.join("\n");
        assert!(joined.contains("coordinator [gone]"), "text:\n{joined}");
        assert!(
            joined.contains("child (4242) [live] [inferred]"),
            "text:\n{joined}"
        );
        let ndjson = super::render_ndjson(&nodes);
        let gone = ndjson
            .iter()
            .find(|row| row.contains("\"lane\":\"coordinator\""))
            .unwrap();
        assert!(gone.contains("\"state\":\"gone\""), "row: {gone}");
        assert!(gone.contains("\"pid\":null"), "row: {gone}");
    }

    /// RECEIPT (pstree). A true root with no parent edge stays a root and is
    /// never inferred from a non-dispatch message.
    #[test]
    fn a_lane_with_no_dispatch_shadow_is_a_root() {
        let mut routes = BTreeMap::new();
        routes.insert("loner".into(), route_with(None));
        let messages = vec![boop::bus::Message {
            kind: "note".into(),
            ..dispatch("whoever", "loner")
        }];
        let edges = super::resolve_edges(&routes, &messages);
        let edge = &edges["loner"];
        assert_eq!(edge.parent, None);
        assert!(!edge.inferred);
    }

    /// RECEIPT (job 2). A route's goal rides the lane line as a ` -- <goal>`
    /// suffix and the ndjson row as a `goal` string.
    #[test]
    fn pstree_carries_the_goal() {
        let mut routes = BTreeMap::new();
        routes.insert(
            "child".into(),
            Route {
                kind: "lane".into(),
                harness: Some("opencode".into()),
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
            },
        );
        let messages = vec![dispatch("coordinator", "child")];
        let edges = super::resolve_edges(&routes, &messages);
        let mut meta = BTreeMap::new();
        meta.insert("child".into(), live_meta(4242));
        let mut include = BTreeSet::new();
        include.insert("child".into());
        let nodes = super::build_lane_nodes(&routes, &edges, &meta, &include);
        let text = super::render_text(&nodes).join("\n");
        assert!(
            text.contains("child (4242) [live] [inferred] -- ship the edge"),
            "text:\n{text}"
        );
        let ndjson = super::render_ndjson(&nodes);
        let row = &ndjson[0];
        assert!(row.contains("\"goal\":\"ship the edge\""), "row: {row}");
    }

    /// RECEIPT (job 2). A lane without a goal renders no text suffix and a
    /// null ndjson goal.
    #[test]
    fn pstree_goal_null_when_absent() {
        let mut routes = BTreeMap::new();
        routes.insert("loner".into(), route_with(None));
        let edges = super::resolve_edges(&routes, &[]);
        let mut meta = BTreeMap::new();
        meta.insert("loner".into(), live_meta(7));
        let mut include = BTreeSet::new();
        include.insert("loner".into());
        let nodes = super::build_lane_nodes(&routes, &edges, &meta, &include);
        let text = super::render_text(&nodes).join("\n");
        assert!(!text.contains(" -- "), "text:\n{text}");
        let row = &super::render_ndjson(&nodes)[0];
        assert!(row.contains("\"goal\":null"), "row: {row}");
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

    /// RECEIPT (boop-doctrine-version-const): failed pre-fix against the
    /// literal "version 10" text once SCHEMA_VERSION moved to 11.
    #[test]
    fn help_text_names_the_current_schema_version() {
        let help = Cli::command().render_long_help().to_string();
        let needle = format!("writes version {}", ident::SCHEMA_VERSION);
        assert!(
            help.contains(&needle),
            "help text missing {needle:?}:\n{help}"
        );
    }

    /// A `ProcReader` that never touches the OS; `queried` proves the caller
    /// went through the trait instead of a concrete `SysinfoSnapshot`.
    struct FakeProcReader {
        queried: std::cell::Cell<bool>,
    }

    impl ProcReader for FakeProcReader {
        fn is_alive(&self, _pid: u32) -> bool {
            self.queried.set(true);
            true
        }
        fn process(&self, pid: u32) -> Option<ProcessInfo> {
            self.queried.set(true);
            Some(ProcessInfo {
                pid,
                parent: None,
                name: "fake".into(),
                command: Vec::new(),
                rss_bytes: 4096,
                cpu_percent: 1.5,
                start_time_secs: 0,
                cwd: None,
            })
        }
        fn children(&self, _pid: u32) -> Vec<u32> {
            Vec::new()
        }
        fn descendants(&self, _pid: u32) -> Vec<u32> {
            Vec::new()
        }
        fn descendant_count(&self, _pid: u32) -> usize {
            0
        }
    }

    /// RECEIPT (boop-procreader-bypass): failed to compile pre-fix, `run_ps_with` did not exist yet.
    #[test]
    fn run_ps_with_drives_the_injected_proc_reader() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        write_route(&dir, "fake-lane", tmux_route("boop-procreader-seam-test")).unwrap();
        let reader = FakeProcReader {
            queried: std::cell::Cell::new(false),
        };
        run_ps_with(Some(&dir), None, true, &reader).unwrap();
        assert!(
            reader.queried.get(),
            "run_ps_with must query the injected ProcReader"
        );
    }

    /// RECEIPT (boop-db-help-blank): failed pre-fix, full blank-about list in commit body.
    #[test]
    fn db_subcommand_tree_has_no_blank_help() {
        fn walk(cmd: &clap::Command, prefix: &str, blank: &mut Vec<String>) {
            for sub in cmd.get_subcommands() {
                let path = format!("{prefix} {}", sub.get_name());
                let empty = sub
                    .get_about()
                    .map(|about| about.to_string().trim().is_empty())
                    .unwrap_or(true);
                if empty {
                    blank.push(path.clone());
                }
                walk(sub, &path, blank);
            }
        }
        let db = DbCmd::augment_subcommands(clap::Command::new("db"));
        let mut blank = Vec::new();
        walk(&db, "db", &mut blank);
        assert!(
            blank.is_empty(),
            "db subcommands with empty about: {blank:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// The two trees. `beep` controls agents, `db` reads what they did; the mapping
// to REST is 1:1 per plans/2026-08-09-boop-openapi.yaml.
// ---------------------------------------------------------------------------

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
enum BeepCmd {
    /// Harness adapters and what each can do.
    Harness {
        #[command(subcommand)]
        cmd: HarnessCmd,
    },
    /// Lanes: the agents boop spawns and tracks.
    Lane {
        #[command(subcommand)]
        cmd: LaneCmd,
    },
    /// Register pane-less coordinators and native subagents.
    Agent {
        #[command(subcommand)]
        cmd: AgentCmd,
    },
    /// Type into a running agent, and say whether the keystrokes landed.
    Hail {
        lane: String,
        #[arg(long)]
        body: String,
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        socket: Option<String>,
        /// Send, then block for the reply exactly as `boop wait <id>` does.
        #[arg(long, value_name = "SECS")]
        wait_timeout: Option<u64>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Mail across lanes.
    Message {
        #[command(subcommand)]
        cmd: MessageCmd,
    },
    /// pid, rss, cpu, uptime, child count per live lane.
    Ps {
        lane: Option<String>,
        /// Include dead routes (no live process behind the pane).
        #[arg(long)]
        all: bool,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Filesystem-style tree of lanes by parent edge.
    Pstree {
        /// Include dead lanes; default is live-only.
        #[arg(long)]
        all: bool,
        #[arg(long, value_enum, default_value_t = PstreeFormat::Text)]
        format: PstreeFormat,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum HarnessCmd {
    List,
    Get { harness: String },
}

#[derive(Subcommand)]
enum HostCmd {
    /// Read one JSON request from stdin and emit one JSON response.
    Chat,
}

#[derive(Subcommand)]
enum LaneCmd {
    /// Every lane, with live or dead.
    List {
        #[arg(long)]
        state: Option<String>,
        #[arg(long)]
        harness: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Make a worktree, spawn the agent, register the route.
    Create {
        /// The lane's whole identity: `feature/<name>`, also fix/, refactor/,
        /// chore/. Lane id and tmux session are the branch with `/` as `-`.
        #[arg(long)]
        branch: Option<String>,
        /// Absolute path to the brief the lane reads and executes.
        #[arg(long)]
        brief: Option<PathBuf>,
        /// What the lane is running toward.
        #[arg(long)]
        goal: Option<String>,
        /// The format the lane's mail is rendered in; it inherits the parent's
        /// when absent.
        #[arg(long)]
        mood: Option<String>,
        /// Continue an existing trace instead of opening one named for the
        /// lane. Every session this lane runs joins it.
        #[arg(long)]
        trace: Option<String>,
        /// Skip the repo's `boop-start` warmup in the new worktree.
        #[arg(long)]
        no_start: bool,
        /// Repo to branch from; defaults to the repo the caller stands in.
        #[arg(long)]
        cwd: Option<String>,
        /// Defaults to origin/main's head, resolved and printed at spawn.
        #[arg(long)]
        base_sha: Option<String>,
        /// Defaults to the caller, then to the one registered coordinator.
        #[arg(long)]
        parent: Option<String>,
        /// What this lane does when its parent route stops answering.
        #[arg(long, value_enum, default_value_t = ParentDeathPolicy::Orphan)]
        on_parent_death: ParentDeathPolicy,
        /// Defaults to the harness the model spelling names.
        #[arg(long)]
        harness: Option<String>,
        #[arg(long)]
        model: Option<String>,
        /// Resolve a named provider/model entry from the platform Boop config.
        #[arg(long, conflicts_with = "model")]
        preset: Option<String>,
        /// opencode reasoning-effort variant (low|medium|high); CLI wins over
        /// the preset's variant, and opencode's default applies when neither.
        #[arg(long)]
        variant: Option<String>,
        /// Block until the lane's on-exit result row lands, then exit with its
        /// rc. Without a parent, the waiter owns a private result recipient.
        #[arg(long)]
        wait: bool,
        /// Seconds `--wait` blocks before exiting 124; 0 waits forever.
        #[arg(long, default_value_t = 3600)]
        wait_timeout: u64,
        /// Overrides the lane id derived from `--branch`.
        #[arg(long)]
        lane: Option<String>,
        /// Overrides the tmux session name derived from `--branch`.
        #[arg(long)]
        tmux: Option<String>,
        /// tmux socket to spawn on; a throwaway socket for tests, `None` for
        /// the default server.
        #[arg(long)]
        socket: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
        /// Remove a dead lane's worktree and branch before spawning. A live
        /// route or a live pane on the name refuses.
        #[arg(long)]
        reclaim: bool,
    },
    /// Drive one lane conversation. This is what a lane pane runs; a human
    /// calls `lane create`, never this.
    Run {
        #[arg(long)]
        lane: String,
        #[arg(long)]
        harness: String,
        /// Absolute path to the brief that opens the conversation.
        #[arg(long)]
        brief: PathBuf,
        #[arg(long)]
        model: Option<String>,
        /// Continue an existing harness conversation instead of opening one.
        #[arg(long)]
        resume: Option<String>,
        /// opencode reasoning-effort variant, threaded from `lane create`.
        #[arg(long)]
        variant: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// One lane's route and state.
    Get {
        lane: String,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Point a lane at a pane that already exists.
    Patch {
        lane: String,
        #[arg(long)]
        tmux: String,
        #[arg(long)]
        harness: Option<String>,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long)]
        cwd: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        mode: Option<String>,
        /// The lane that summoned this one.
        #[arg(long)]
        parent: Option<String>,
        #[arg(long)]
        goal: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Stop a lane and forget it, or bulk-delete by state.
    Delete {
        lane: Option<String>,
        /// Drop only the registry route; never kill the pane. The `--parent`
        /// on-exit epilogue uses this to clean up while still running inside it.
        #[arg(long)]
        route_only: bool,
        #[arg(long)]
        state: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Drop routes whose tmux session is gone AND whose recorded pid, if any,
    /// is not alive. Refuses when tmux is unreachable.
    Prune {
        /// Print what would be pruned; remove nothing.
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Which tmux pane and harness session id.
    Route {
        lane: String,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Show the lane's screen.
    Pane {
        lane: String,
        #[arg(long)]
        lines: Option<u32>,
        #[arg(long)]
        socket: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// The lane's mailbox.
    Message {
        #[command(subcommand)]
        cmd: LaneMessageCmd,
    },
    /// Wait for the lane's result row, then exit with the rc it names. `--timeout`
    /// seconds exits 124; a route that dies with no row exits 3.
    Wait {
        lane: String,
        /// Seconds to wait before exiting 124; 0 waits until the lane reports
        /// or its route dies.
        #[arg(long, default_value_t = 0)]
        timeout: u64,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum AgentCmd {
    /// Add a pane-less registry row.
    Register {
        name: String,
        #[arg(long, default_value = "native")]
        kind: String,
        #[arg(long)]
        parent: Option<String>,
        /// Recorded for this row; a pane-less agent runs no supervisor of its
        /// own, so nothing polls on its behalf.
        #[arg(long, value_enum, default_value_t = ParentDeathPolicy::Orphan)]
        on_parent_death: ParentDeathPolicy,
        /// The tree this agent works in. Warmed like a lane spawn's worktree,
        /// with the preamble printed here: a native has no injected first turn.
        #[arg(long)]
        worktree: Option<PathBuf>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Append a completion row and remove the registry row.
    Done {
        name: String,
        #[arg(long, default_value_t = 0)]
        rc: i32,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
}

#[cfg(feature = "agent-read")]
#[derive(Subcommand)]
enum AgentSummaryCmd {
    /// Synchronize incremental transcript facts, then emit the versioned
    /// CASS-compatible Boop agent summary.
    Summary {
        #[arg(long, value_enum, default_value_t = AgentSummaryFormat::Json)]
        format: AgentSummaryFormat,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Synchronize transcripts, then emit the native session graph.
    Sessions {
        /// Restrict session and shell rows to one working directory.
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Include historical and inactive rows.
        #[arg(long)]
        history: bool,
        /// Restrict the graph to the family connected to this tmux session or pane.
        #[arg(long)]
        tmux: Option<String>,
        /// Include historical family rows active at or after this Unix timestamp in milliseconds.
        #[arg(long)]
        history_since_ts: Option<u64>,
        /// The public graph contract currently emits JSON.
        #[arg(long, value_enum, default_value_t = AgentSessionGraphFormat::Json)]
        format: AgentSessionGraphFormat,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
}

#[cfg(feature = "agent-read")]
#[derive(Clone, Copy, ValueEnum)]
enum AgentSessionGraphFormat {
    Json,
}

#[derive(Subcommand)]
enum LaneMessageCmd {
    List {
        lane: String,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
enum HookArg {
    /// Claude Code's `Stop`: the mail comes back as a block decision.
    Stop,
    /// Claude Code's `UserPromptSubmit`: the mail is printed as context.
    Prompt,
    /// A human or a script reading the inbox.
    Plain,
}

impl From<HookArg> for boop::inbox::Hook {
    fn from(arg: HookArg) -> boop::inbox::Hook {
        match arg {
            HookArg::Stop => boop::inbox::Hook::Stop,
            HookArg::Prompt => boop::inbox::Hook::Prompt,
            HookArg::Plain => boop::inbox::Hook::Plain,
        }
    }
}

#[derive(Subcommand)]
enum InboxCmd {
    /// Print the unread mail addressed to a coordinator and record it as handed
    /// over. Silent with an empty inbox, so a hook that runs on every turn
    /// costs one line of nothing.
    Drain {
        /// Whose inbox to drain; defaults to the identity ladder's answer.
        #[arg(long = "as", value_name = "NAME")]
        as_name: Option<String>,
        #[arg(long, value_enum, default_value_t = HookArg::Plain)]
        hook: HookArg,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Install (or remove) the two drain hooks in <cwd>/.claude/settings.json.
    /// `boop adopt --harness claude` does this for you.
    Hooks {
        #[arg(long)]
        name: String,
        /// The project whose settings carry the hooks; defaults to this dir.
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(long)]
        uninstall: bool,
    },
}

#[derive(Subcommand)]
enum MessageCmd {
    /// Mark mail handled, in bulk.
    Ack {
        #[arg(long)]
        lane: Option<String>,
        #[arg(long)]
        box_: Option<String>,
        #[arg(long)]
        close_routeless: bool,
        #[arg(long, default_value_t = 7)]
        max_age_days: u64,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum DbCmd {
    /// Versioned CASS-compatible agent/runtime/activity summary. CASS issue,
    /// reservation, and provider records are separate contracts.
    #[cfg(feature = "agent-read")]
    #[command(hide = true)]
    AgentSummary {
        #[arg(long, value_enum, default_value_t = AgentSummaryFormat::Json)]
        format: AgentSummaryFormat,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Rows from `agent_session`: one row per transcript session.
    #[cfg(feature = "agent-read")]
    Session {
        #[command(subcommand)]
        cmd: SessionCmd,
    },
    /// Rows from `agent_turn`: one row per user/assistant turn.
    Turn {
        #[command(subcommand)]
        cmd: TurnCmd,
    },
    /// `agent_turn` projected into NDJSON chat-repr turns.
    Chat {
        #[command(subcommand)]
        cmd: ChatCmd,
    },
    /// Rows from `agent_touch`: files a session read or edited.
    #[cfg(feature = "agent-read")]
    Touch {
        #[command(subcommand)]
        cmd: FactCmd,
    },
    /// Rows from `agent_cmd`: shell commands a session ran.
    #[cfg(feature = "agent-read")]
    Command {
        #[command(subcommand)]
        cmd: FactCmd,
    },
    /// Rows from `agent_fetch`: URLs a session fetched.
    #[cfg(feature = "agent-read")]
    Fetch {
        #[command(subcommand)]
        cmd: FactCmd,
    },
    /// Rows from `agent_skill`: skills a session invoked.
    #[cfg(feature = "agent-read")]
    Skill {
        #[command(subcommand)]
        cmd: FactCmd,
    },
    /// Rows from `agent_pr`: pull requests a session touched.
    #[cfg(feature = "agent-read")]
    Pr {
        #[command(subcommand)]
        cmd: FactCmd,
    },
    /// Rows from `agent_span`: live time spans a session recorded.
    #[cfg(feature = "agent-read")]
    Span {
        #[command(subcommand)]
        cmd: FactCmd,
    },
    /// Rows from `agent_edge`: parent/child spawn edges between sessions.
    Edge {
        #[command(subcommand)]
        cmd: EdgeCmd,
    },
    /// Tokens and cost. A totals report the passthrough powers, and a parent
    /// of the row computations blocks and burn-rate; clap needs both attributes
    /// to accept the two forms.
    #[cfg(feature = "agent-read")]
    #[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
    Usage {
        #[command(flatten)]
        args: UsageArgs,
        /// Print this alias's SQL and exit.
        #[arg(long)]
        show_sql: bool,
        #[command(subcommand)]
        cmd: Option<UsageCmd>,
    },
    /// The rate table cost is computed from.
    #[cfg(feature = "agent-read")]
    Price {
        #[command(subcommand)]
        cmd: PriceCmd,
    },
    /// User-pinned markdown: save a message you want to keep, read it back.
    #[cfg(feature = "agent-read")]
    Favorite {
        #[command(subcommand)]
        cmd: FavoriteCmd,
    },
    /// Ingest new transcript bytes.
    Sync {
        #[command(subcommand)]
        cmd: SyncCmd,
    },
    /// How far ingest has read each transcript.
    #[cfg(feature = "agent-read")]
    SyncCursor {
        #[command(subcommand)]
        cmd: CursorCmd,
    },
    /// Who is alive, who moved recently, and what it cost.
    #[cfg(feature = "agent-read")]
    Status {
        /// Window in minutes.
        #[arg(long, default_value_t = 10)]
        window: u64,
        #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
        format: QueryFormat,
    },
}

#[cfg(feature = "agent-read")]
#[derive(clap::Args, Clone, Default)]
struct UsageArgs {
    #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
    format: QueryFormat,
}

#[cfg(feature = "agent-read")]
#[derive(clap::Args, Clone, Default)]
struct FactArgs {
    #[arg(long)]
    session: Option<String>,
    #[arg(long)]
    since: Option<u64>,
    #[arg(long)]
    until: Option<u64>,
    /// Prefix match on the row's leading dictionary column.
    #[arg(long)]
    like: Option<String>,
    #[arg(long)]
    limit: Option<u64>,
    #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
    format: QueryFormat,
}

#[cfg(feature = "agent-read")]
#[derive(Subcommand)]
enum UsageCmd {
    /// Gap-aware billing windows.
    Blocks {
        #[arg(long, default_value_t = 5)]
        window_hours: u64,
        /// Only the window that is still open.
        #[arg(long)]
        active: bool,
        #[command(flatten)]
        args: UsageArgs,
    },
    /// Tokens per minute and dollars per hour over a trailing window.
    BurnRate {
        #[arg(long, default_value_t = 60)]
        window_minutes: u64,
        #[command(flatten)]
        args: UsageArgs,
    },
}

#[cfg(feature = "agent-read")]
#[derive(Subcommand)]
enum PriceCmd {
    /// Every rate row in `model_price`.
    List,
    /// Write one rate row by hand, in USD per million tokens.
    Set {
        model: String,
        #[arg(long)]
        input_per_mtok: f64,
        #[arg(long)]
        output_per_mtok: f64,
        #[arg(long)]
        cache_write_5m_per_mtok: f64,
        #[arg(long)]
        cache_write_1h_per_mtok: f64,
        #[arg(long)]
        cache_read_per_mtok: f64,
        #[arg(long, default_value = "manual")]
        source: String,
    },
}

#[cfg(feature = "agent-read")]
#[derive(Subcommand)]
enum FactCmd {
    /// The fact rows for this kind's table, filtered by `FactArgs`.
    List {
        #[command(flatten)]
        args: FactArgs,
    },
}

#[cfg(feature = "agent-read")]
#[derive(Subcommand)]
enum SessionCmd {
    /// Every session row from `agent_session`, newest first.
    List {
        #[arg(long)]
        limit: Option<u64>,
        #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
        format: QueryFormat,
    },
    /// The one `agent_session` row matching this session id.
    Get {
        session: String,
        #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
        format: QueryFormat,
    },
}

#[derive(Subcommand)]
enum TurnCmd {
    /// Every `agent_turn` row matching `QueryArgs`.
    List {
        #[command(flatten)]
        query: QueryArgs,
    },
    /// The one `agent_turn` row at this session and turn number.
    Get {
        session: String,
        turn: u64,
        #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
        format: QueryFormat,
    },
}

#[derive(Subcommand)]
enum ChatCmd {
    /// `agent_turn` rows projected into chat-repr NDJSON turns.
    List {
        #[command(flatten)]
        query: QueryArgs,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        follow: bool,
    },
}

#[derive(Subcommand)]
enum EdgeCmd {
    /// Every `agent_edge` row, filtered to one session's edges when given.
    List {
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        limit: Option<u64>,
    },
}

#[derive(Subcommand)]
enum SyncCmd {
    /// Ingest new transcript bytes into the store's `agent_turn` and fact tables.
    Create {
        #[arg(long)]
        rebuild: bool,
        /// Keep syncing on a poll instead of returning.
        #[arg(long)]
        forever: bool,
    },
}

#[derive(Subcommand)]
enum MeCmd {
    /// Read or set the format agents mail this session in. No name prints the
    /// effective mood and the session that set it.
    Mood {
        /// A stored mood name; `boop db "select * from mood"` lists them.
        #[arg(conflicts_with = "clear")]
        name: Option<String>,
        /// Drop this session's own mood row, so it inherits again.
        #[arg(long)]
        clear: bool,
        /// The session to act on; defaults to the caller.
        #[arg(long = "as", value_name = "SESSION")]
        as_name: Option<String>,
    },
    /// Save one assistant turn from the caller's conversation as a favorite.
    Favorite {
        /// Assistant turn position: -1 is newest, -2 is the one before it.
        #[arg(default_value_t = -1, allow_hyphen_values = true)]
        index: i64,
        /// Why this message is kept.
        #[arg(long)]
        note: Option<String>,
    },
}

#[cfg(feature = "agent-read")]
#[derive(Subcommand)]
enum FavoriteCmd {
    /// Pin markdown into the store, from --file or stdin.
    Add {
        /// Markdown file to pin; stdin when absent.
        #[arg(long)]
        file: Option<PathBuf>,
        /// Why this one is kept.
        #[arg(long)]
        note: Option<String>,
        /// Where it came from: a session id, a url, plain text.
        #[arg(long)]
        source: Option<String>,
    },
    /// Favorites newest-first, body included.
    List {
        #[arg(long)]
        limit: Option<u64>,
        #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
        format: QueryFormat,
    },
}

#[cfg(feature = "agent-read")]
#[derive(Subcommand)]
enum CursorCmd {
    /// Every `sync_cursor` row: how far ingest has read each transcript.
    List {
        #[arg(long)]
        limit: Option<u64>,
        #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
        format: QueryFormat,
    },
}

// ---------------------------------------------------------------------------
// beep
// ---------------------------------------------------------------------------

fn run_beep(registry: &Registry, cmd: BeepCmd) -> Result<()> {
    match cmd {
        BeepCmd::Harness { cmd } => match cmd {
            HarnessCmd::List => run_harnesses(registry),
            HarnessCmd::Get { harness } => run_harness_get(registry, &harness),
        },
        BeepCmd::Lane { cmd } => run_beep_lane(registry, cmd),
        BeepCmd::Agent { cmd } => run_agent(cmd),
        BeepCmd::Hail {
            lane,
            body,
            from,
            kind,
            socket,
            wait_timeout,
            mail_dir,
        } => run_hail(
            registry,
            &lane,
            &body,
            from.as_deref(),
            kind.as_deref(),
            None,
            socket.as_deref(),
            wait_timeout,
            mail_dir.as_deref(),
        ),
        BeepCmd::Message { cmd } => match cmd {
            MessageCmd::Ack {
                lane,
                box_,
                close_routeless,
                max_age_days,
                mail_dir,
            } => run_sweep(
                mail_dir.as_deref(),
                box_.as_deref(),
                lane.as_deref(),
                close_routeless,
                max_age_days,
            ),
        },
        BeepCmd::Ps {
            lane,
            all,
            mail_dir,
        } => run_ps(mail_dir.as_deref(), lane.as_deref(), all),
        BeepCmd::Pstree {
            all,
            format,
            mail_dir,
        } => run_pstree(mail_dir.as_deref(), all, format),
    }
}

fn run_agent(cmd: AgentCmd) -> Result<()> {
    match cmd {
        AgentCmd::Register {
            name,
            kind,
            parent,
            on_parent_death,
            worktree,
            mail_dir: mail_dir_arg,
        } => {
            if !matches!(kind.as_str(), "coordinator" | "native") {
                anyhow::bail!("agent kind must be coordinator or native")
            }
            if let Some(tree) = worktree.as_deref().filter(|tree| !tree.is_dir()) {
                anyhow::bail!("no worktree at {}", tree.display());
            }
            let dir = mail_dir(mail_dir_arg.as_deref())?;
            boop::supervise::record_parent_policy(&dir, &name, on_parent_death)?;
            let started = worktree
                .as_deref()
                .map(boop::worktree::warm_start)
                .transpose()?;
            write_route(
                &dir,
                &name,
                Route {
                    kind,
                    harness: None,
                    tmux: None,
                    cwd: worktree.as_ref().map(|dir| dir.display().to_string()),
                    model: None,
                    mode: None,
                    session_id: None,
                    source_path: None,
                    parent,
                    goal: None,
                    registered_at: Some(bus::now_iso()),
                    base_sha: None,
                    worktree_dir: worktree.as_ref().map(|dir| dir.display().to_string()),
                },
            )?;
            println!("registered {name}");
            if let Some(outcome) = started {
                print!("{}", boop::lane::start_preamble(&outcome.status));
            }
            Ok(())
        }
        AgentCmd::Done {
            name,
            rc,
            mail_dir: mail_dir_arg,
        } => {
            let dir = mail_dir(mail_dir_arg.as_deref())?;
            let routes = bus::read_routes(&dir)?;
            let route = routes
                .get(&name)
                .with_context(|| format!("no registered native route for `{name}`"))?;
            if !matches!(route.kind.as_str(), "coordinator" | "native") {
                anyhow::bail!("route `{name}` is not a native agent route")
            }
            let parent = route
                .parent
                .as_deref()
                .unwrap_or("sprefa-coordinator")
                .to_owned();
            let message = bus::Message {
                id: bus::mint_id(),
                from: name.clone(),
                to: parent,
                from_timestamp: bus::now_iso(),
                to_timestamp: None,
                kind: "result".into(),
                reply_to: None,
                body: format!("lane {name} done rc={rc}"),
                r#ref: None,
                rc: Some(rc),
                detail: None,
            };
            append_message(&dir, &message)?;
            let path = dir.join("registry.json");
            bus::cas_update_json(&path, |current| {
                current.remove(&name);
                Ok(())
            })?;
            println!("{}", message.body);
            Ok(())
        }
    }
}

fn run_beep_lane(registry: &Registry, cmd: LaneCmd) -> Result<()> {
    match cmd {
        LaneCmd::List {
            state,
            harness,
            mail_dir,
        } => run_lane_list(mail_dir.as_deref(), state.as_deref(), harness.as_deref()),
        LaneCmd::Create {
            lane,
            cwd,
            harness,
            brief,
            model,
            preset,
            variant,
            tmux,
            parent,
            branch,
            base_sha,
            socket,
            goal,
            trace,
            no_start,
            mail_dir,
            dry_run,
            wait,
            wait_timeout,
            mood,
            reclaim,
            on_parent_death,
        } => {
            // Recorded before the spawn: the route the dispatch writes replaces
            // whatever is under this lane's key.
            if !dry_run {
                boop::supervise::record_spawn_policy(
                    &crate::mail_dir(mail_dir.as_deref())?,
                    branch.as_deref(),
                    lane.as_deref(),
                    on_parent_death,
                )?;
            }
            run_lane(
                registry,
                LaneArgs {
                    name: lane,
                    cwd,
                    harness,
                    brief,
                    model,
                    preset,
                    variant,
                    tmux,
                    parent,
                    branch,
                    base_sha,
                    socket,
                    goal,
                    mood,
                    trace,
                    no_start,
                    mail_dir,
                    dry_run,
                    wait,
                    wait_timeout,
                    reclaim,
                },
            )
        }
        LaneCmd::Run {
            lane,
            harness,
            brief,
            model,
            resume,
            variant,
            mail_dir,
        } => run_lane_supervisor(
            registry,
            &lane,
            &harness,
            &brief,
            model.as_deref(),
            resume.as_deref(),
            variant.as_deref(),
            mail_dir.as_deref(),
        ),
        LaneCmd::Get { lane, mail_dir } => run_lane_get(mail_dir.as_deref(), &lane),
        LaneCmd::Patch {
            lane,
            tmux,
            harness,
            session_id,
            cwd,
            model,
            mode,
            parent,
            goal,
            mail_dir,
            // A lane pane runs a supervisor that reads the mailbox itself, so
            // no hook inbox belongs on it.
        } => run_adopt(
            &lane,
            "lane",
            &tmux,
            harness.as_deref(),
            session_id.as_deref(),
            cwd.as_deref(),
            model.as_deref(),
            mode.as_deref(),
            parent.as_deref(),
            goal.as_deref(),
            mail_dir.as_deref(),
            HookWiring {
                no_hooks: true,
                uninstall: false,
            },
        ),
        LaneCmd::Delete {
            lane,
            route_only,
            state,
            mail_dir,
        } => match (lane, state) {
            (Some(lane), _) => run_lane_delete(mail_dir.as_deref(), &lane, route_only),
            (None, Some(_)) => run_prune(mail_dir.as_deref()),
            (None, None) => {
                anyhow::bail!("name a lane to delete, or pass --state dead for a bulk delete")
            }
        },
        LaneCmd::Prune { dry_run, mail_dir } => run_lane_prune(mail_dir.as_deref(), dry_run),
        LaneCmd::Route { lane, mail_dir } => run_resolve(&lane, mail_dir.as_deref()),
        LaneCmd::Pane {
            lane,
            lines,
            socket,
            mail_dir,
        } => run_lane_pane(mail_dir.as_deref(), &lane, lines, socket.as_deref()),
        LaneCmd::Message { cmd } => match cmd {
            LaneMessageCmd::List { lane, mail_dir } => {
                run_list(mail_dir.as_deref(), Some(&lane), true)
            }
        },
        LaneCmd::Wait {
            lane,
            timeout,
            mail_dir,
        } => run_lane_wait(mail_dir.as_deref(), &lane, timeout),
    }
}

fn run_harness_get(registry: &Registry, id: &str) -> Result<()> {
    let adapter = resolve_harness(registry, id)?;
    let caps = adapter.capabilities();
    println!(
        "{}",
        serde_json::json!({
            "harness": adapter.id(),
            "send_midflight": caps.send_midflight,
            "resume": caps.resume,
            "spawn": caps.spawn,
            "subagent_visible": caps.subagent_visible,
        })
    );
    Ok(())
}

/// Lanes only. `boop list` printed routes and mail together; the two trees
/// split that, so this half never prints a message.
fn run_lane_list(
    mail_dir_arg: Option<&Path>,
    state_filter: Option<&str>,
    harness_filter: Option<&str>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let live = tmux::mux().live_sessions(None);
    for (name, route) in &routes {
        let state = lane_state(&live, route);
        if let Some(want) = state_filter {
            if state != want {
                continue;
            }
        }
        if let Some(want) = harness_filter {
            if route.harness.as_deref() != Some(want) {
                continue;
            }
        }
        let flags = escape_flags(&dir, name);
        let mut suffix = String::new();
        if state == "dead" {
            suffix.push_str(&format!(" DEAD={}", dead_reason_token(&dir, name)));
        }
        if let Some(gone) = gone_parent(&routes, &live, route) {
            suffix.push_str(&format!(" PARENT-GONE={gone}"));
        }
        if let Some(flags) = &flags {
            if flags.worktree_untouched {
                suffix.push_str(" WORKTREE-UNTOUCHED");
            }
            if !flags.main_commits.is_empty() {
                suffix.push_str(&format!(
                    " MAIN-TREE-COMMIT-SUSPECT={}",
                    flags.main_commits.join(",")
                ));
            }
            for commit in &flags.ambiguous_main_commits {
                suffix.push_str(&format!(
                    " MAIN-TREE-COMMIT-AMBIGUOUS={}:{}",
                    commit.sha,
                    commit.lanes.join("|")
                ));
            }
        }
        line(&format!(
            "{} {} {} {} {} {} {} {}{}",
            pad(state, 4),
            pad(name, 16),
            pad(&route.kind, 12),
            pad(route.harness.as_deref().unwrap_or("-"), 10),
            pad(route.mode.as_deref().unwrap_or("-"), 6),
            pad(route.model.as_deref().unwrap_or("-"), 46),
            pad(route.tmux.as_deref().unwrap_or("-"), 16),
            route.cwd.as_deref().unwrap_or("-"),
            suffix,
        ));
    }
    Ok(())
}

/// The parent edge that answers nobody, so a surviving orphan says so on its
/// own row. `None` while the parent route is still addressable.
fn gone_parent<'a>(
    routes: &BTreeMap<String, Route>,
    live: &Option<tmux::LiveSessions>,
    route: &'a Route,
) -> Option<&'a str> {
    let parent = route.parent.as_deref()?;
    match routes.get(parent) {
        Some(parent_route) if lane_state(live, parent_route) != "dead" => None,
        _ => Some(parent),
    }
}

/// Why a dead lane is dead, as one token. A missing home directory is itself an
/// answer: nothing could have been written, so the row says `no-trail`.
fn dead_reason_token(mail_dir: &std::path::Path, lane: &str) -> String {
    let Ok(root) = boop::trail::lanes_root() else {
        return boop::trail::DeadReason::NoTrail.token();
    };
    boop::trail::dead_reason(mail_dir, &root, lane).token()
}

fn lane_state(live: &Option<tmux::LiveSessions>, route: &Route) -> &'static str {
    // Pane-less native registrations are addressable for their entire
    // registration lifetime. Their completion event is `agent done`, so an
    // absent tmux or process trail carries no death information.
    if route.tmux.is_none() && matches!(route.kind.as_str(), "coordinator" | "native") {
        return "live";
    }
    match live {
        None => "?",
        Some(_)
            if route
                .tmux
                .as_deref()
                .is_some_and(|target| tmux::mux().target_alive(None, target)) =>
        {
            "live"
        }
        Some(_) => "dead",
    }
}

fn run_lane_get(mail_dir_arg: Option<&Path>, lane: &str) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let Some(route) = routes.get(lane) else {
        anyhow::bail!("no registry route for lane `{lane}`")
    };
    let live = tmux::mux().live_sessions(None);
    println!(
        "{}",
        serde_json::json!({
            "lane": lane,
            "state": lane_state(&live, route),
            "harness": route.harness,
            "tmux": route.tmux,
            "cwd": route.cwd,
            "model": route.model,
            "mode": route.mode,
            "session_id": route.session_id,
        })
    );
    Ok(())
}

/// Stop one lane and drop its route. Refuses when tmux is unreachable. `--route-only`
/// drops the registry row and never touches the pane, so the on-exit epilogue can run inside it.
fn run_lane_delete(mail_dir_arg: Option<&Path>, lane: &str, route_only: bool) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let Some(route) = routes.get(lane) else {
        if route_only {
            anyhow::bail!("no registry route for lane `{lane}`")
        }
        return run_lane_delete_carcass(lane);
    };
    if !route_only {
        if let Some(session) = route.tmux.as_deref() {
            match tmux::mux().has_session(None, session) {
                Ok(true) => tmux::mux().kill_session(None, session)?,
                Ok(false) => {}
                Err(error) => anyhow::bail!("tmux unreachable, refusing to delete {lane}: {error}"),
            }
        }
    }
    let path = dir.join("registry.json");
    bus::cas_update_json(&path, |current| {
        current.remove(lane);
        Ok(())
    })?;
    info!(lane, route_only, "lane route deleted");
    println!("deleted {lane}");
    Ok(())
}

/// A DOA spawn's epilogue drops the route before the driver can delete the
/// lane, so the worktree and branch are all that is left to remove.
fn run_lane_delete_carcass(lane: &str) -> Result<()> {
    let here = std::env::current_dir().context("read the current directory")?;
    let repo = lane::repo_root(&here)?;
    let removed =
        lane::delete_carcass(&repo, lane, |target| tmux::mux().target_alive(None, target))?;
    for line in removed.lines() {
        println!("deleted {lane}: {line}");
    }
    if removed.nothing_removed() {
        println!("deleted {lane}: nothing left to remove");
    }
    info!(lane, "lane carcass deleted");
    Ok(())
}

/// Bulk-drop dead rows. Registry-only: bus.ndjson is never touched.
fn run_lane_prune(mail_dir_arg: Option<&Path>, dry_run: bool) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    if tmux::mux().live_sessions(None).is_none() {
        anyhow::bail!("tmux unreachable, cannot tell live from dead");
    }
    let routes = bus::read_routes(&dir)?;
    let snapshot = proc::SysinfoSnapshot::capture()?;
    let dead: Vec<(String, String, String)> = routes
        .iter()
        .filter(|(_, route)| route.kind == "lane")
        .filter_map(|(name, route)| {
            let why = dead_reason(route, &snapshot)?;
            Some((
                name.clone(),
                route.tmux.clone().unwrap_or_else(|| "-".into()),
                why,
            ))
        })
        .collect();
    for (name, tmux_name, why) in &dead {
        line(&format!("lane {name} {tmux_name} {why}"));
    }
    if dry_run {
        line(&format!("{} lane(s) would be pruned (dry run)", dead.len()));
        return Ok(());
    }
    let path = dir.join("registry.json");
    bus::cas_update_json(&path, |current| {
        for (name, _, _) in &dead {
            current.remove(name);
        }
        Ok(())
    })?;
    line(&format!("{} lane(s) pruned", dead.len()));
    Ok(())
}

/// `None` when the route's tmux target is live; `Some(reason)` when the tmux
/// target is gone and its resolvable pid, if any, is also not alive.
fn dead_reason(route: &Route, snapshot: &proc::SysinfoSnapshot) -> Option<String> {
    let Some(target) = route.tmux.as_deref() else {
        return Some("no tmux session recorded".to_owned());
    };
    if tmux::mux().target_alive(None, target) {
        return None;
    }
    match tmux::mux().pane_pid(None, target) {
        Some(pid) if snapshot.is_alive(pid) => None,
        Some(pid) => Some(format!("tmux session gone, pid {pid} not alive")),
        None => Some("tmux session gone, no pid recorded".to_owned()),
    }
}

fn run_lane_pane(
    mail_dir_arg: Option<&Path>,
    lane: &str,
    lines: Option<u32>,
    socket: Option<&str>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let Some(route) = routes.get(lane) else {
        anyhow::bail!("no registry route for lane `{lane}`")
    };
    let Some(target) = route.tmux.as_deref() else {
        anyhow::bail!("lane `{lane}` has no tmux session to capture")
    };
    print!("{}", tmux::mux().capture_pane(socket, target, lines)?);
    Ok(())
}

/// `beep lane wait`: poll for a `kind=result` row from `lane`, exit with its
/// rc; `--timeout` seconds exits 124, a pre-existing row returns immediately.
/// A route that goes dead with no result row exits 3.
fn run_lane_wait(mail_dir_arg: Option<&Path>, lane: &str, timeout_secs: u64) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let deadline = if timeout_secs == 0 {
        None
    } else {
        Some(std::time::Duration::from_secs(timeout_secs))
    };
    info!(lane, timeout_secs, "lane result wait starting");
    match wait_for_outcome(
        &dir,
        lane,
        deadline,
        std::time::Duration::from_secs(1),
        &route_liveness,
    ) {
        WaitOutcome::Result(rc) => {
            info!(lane, exit_code = rc, "lane result received");
            if let Some(flags) = escape_flags(&dir, lane) {
                print_escape_flags(lane, &flags);
            }
            std::process::exit(rc)
        }
        WaitOutcome::Died => {
            warn!(lane, exit_code = 3, "lane route died with no result row");
            line(&format!(
                "lane {lane} died without a result (see its worktree and opencode session for the trail)"
            ));
            std::process::exit(3)
        }
        WaitOutcome::TimedOut => {
            info!(lane, exit_code = 124, "lane result wait timed out");
            std::process::exit(124)
        }
    }
}

/// What one wait resolved to.
#[derive(Debug, PartialEq, Eq)]
enum WaitOutcome {
    Result(i32),
    /// The route stopped being live and no result row for this spawn exists.
    Died,
    TimedOut,
}

/// Whether the lane's route is still backed by a live tmux session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RouteLiveness {
    Live,
    Dead,
    /// No route row, no tmux target, or tmux itself unreachable. None of the
    /// three is evidence of death, so none of them ends a wait.
    Unknown,
}

/// Consecutive dead observations before a wait calls the lane dead. A route is
/// written before its session answers, so one observation is never enough.
const DEAD_POLLS: u32 = 5;

/// The route's liveness through the same probe `lane list` prints.
fn route_liveness(dir: &std::path::Path, lane: &str) -> RouteLiveness {
    let Ok(routes) = bus::read_routes(dir) else {
        return RouteLiveness::Unknown;
    };
    let Some(route) = routes.get(lane) else {
        return RouteLiveness::Unknown;
    };
    if route.tmux.is_none() && matches!(route.kind.as_str(), "coordinator" | "native") {
        return RouteLiveness::Live;
    }
    if route.tmux.is_none() {
        return RouteLiveness::Unknown;
    }
    match lane_state(&tmux::mux().live_sessions(None), route) {
        "live" => RouteLiveness::Live,
        "dead" => RouteLiveness::Dead,
        _ => RouteLiveness::Unknown,
    }
}

/// The rc from the lane's most recent `kind=result` mailbox row (`lane <id>
/// done rc=N`), `None` when no result row for that lane exists yet. Without a
/// route row the wait is after-the-fact: any result row satisfies.
#[cfg(test)]
fn lane_result_rc(dir: &std::path::Path, lane: &str) -> Option<i32> {
    lane_result_rc_since(dir, lane, None)
}

/// As `lane_result_rc`, but only a result row at or after `since` (ms since
/// epoch) satisfies; older rows belong to a previous spawn and are skipped.
fn lane_result_rc_since(dir: &std::path::Path, lane: &str, since: Option<u64>) -> Option<i32> {
    let mut messages = Vec::new();
    for box_path in bus::read_boxes(dir).unwrap_or_default() {
        messages.extend(bus::parse_box(&box_path));
    }
    let folded = bus::fold(&messages);
    folded
        .iter()
        .rev()
        // The supervisor hails `--to <parent> --from <lane>`, so the lane that
        // finished is the sender; `to` matches a hand-addressed row.
        .find(|message| {
            if message.kind != "result" || (message.from != lane && message.to != lane) {
                return false;
            }
            match since {
                Some(boundary) => parse_iso_ms(&message.from_timestamp).unwrap_or(0) >= boundary,
                None => true,
            }
        })
        .and_then(|message| message.rc)
}

/// The lane's registration timestamp (ms since epoch) for the spawn that
/// wrote the current route row; `None` when no route row exists.
fn route_registered_at(dir: &std::path::Path, lane: &str) -> Option<u64> {
    bus::read_routes(dir)
        .ok()?
        .get(lane)
        .and_then(|route| route.registered_at.as_deref())
        .and_then(parse_iso_ms)
}

/// The worktree-escape flags for a lane, or `None` when the route records no
/// worktree (a main-tree spawn) or no base sha to compare against.
fn escape_flags(dir: &std::path::Path, lane: &str) -> Option<boop::worktree::EscapeFlags> {
    let routes = bus::read_routes(dir).ok()?;
    let route = routes.get(lane)?;
    let worktree = std::path::Path::new(route.worktree_dir.as_deref()?);
    let base_sha = route.base_sha.as_deref()?;
    if base_sha.is_empty() {
        return None;
    }
    let repo = lane::repo_root(worktree).ok()?;
    let run = lane_window(dir, lane, &routes)?;
    // Every other lane registered against this same repo. Without them a shared
    // main tree makes one lane's commits look like every lane's.
    let siblings: Vec<boop::worktree::LaneWindow> = routes
        .keys()
        .filter(|name| name.as_str() != lane)
        .filter(|name| sibling_repo(name, &routes).as_deref() == Some(repo.as_path()))
        .filter_map(|name| lane_window(dir, name, &routes))
        .collect();
    Some(boop::worktree::detect_escape(
        worktree, &repo, base_sha, &run, &siblings,
    ))
}

/// The repo a lane's registered worktree belongs to.
fn sibling_repo(
    lane: &str,
    routes: &std::collections::BTreeMap<String, Route>,
) -> Option<std::path::PathBuf> {
    let route = routes.get(lane)?;
    lane::repo_root(std::path::Path::new(route.worktree_dir.as_deref()?)).ok()
}

/// When a lane held its repo and on which branch: the spawn's `registered_at`
/// opens the window, its result row closes it, and the worktree names the
/// branch that witnesses reachability.
fn lane_window(
    dir: &std::path::Path,
    lane: &str,
    routes: &std::collections::BTreeMap<String, Route>,
) -> Option<boop::worktree::LaneWindow> {
    let route = routes.get(lane)?;
    let worktree = std::path::Path::new(route.worktree_dir.as_deref()?);
    let start_ms = route.registered_at.as_deref().and_then(parse_iso_ms)?;
    Some(boop::worktree::LaneWindow {
        lane: lane.to_owned(),
        branch: boop::worktree::current_branch(worktree),
        start_secs: (start_ms / 1000) as i64,
        end_secs: lane_result_at_ms(dir, lane, start_ms).map(|ms| (ms / 1000) as i64),
    })
}

/// Epoch millis of the lane's newest result row at or after `since`, which is
/// the moment the lane stopped being able to commit anywhere.
fn lane_result_at_ms(dir: &std::path::Path, lane: &str, since: u64) -> Option<u64> {
    let mut rows = Vec::new();
    for path in bus::read_boxes(dir).unwrap_or_default() {
        rows.extend(bus::parse_box(&path));
    }
    rows.iter()
        .filter(|row| row.kind == "result" && row.from == lane)
        .filter_map(|row| parse_iso_ms(&row.from_timestamp))
        .filter(|written| *written >= since)
        .max()
}

/// Print the loud escape flags to stdout. `WORKTREE-UNTOUCHED` names a lane
/// whose worktree gained no commit; `MAIN-TREE-COMMIT-SUSPECT` lists the shas
/// only this lane's branch or window accounts for, and
/// `MAIN-TREE-COMMIT-AMBIGUOUS` the shas a concurrent lane could equally have
/// made.
fn print_escape_flags(lane: &str, flags: &boop::worktree::EscapeFlags) {
    if flags.worktree_untouched {
        println!("WORKTREE-UNTOUCHED {lane}: no new commits in its registered worktree");
    }
    if !flags.main_commits.is_empty() {
        println!(
            "MAIN-TREE-COMMIT-SUSPECT {lane}: {}",
            flags.main_commits.join(" ")
        );
    }
    for commit in &flags.ambiguous_main_commits {
        println!(
            "MAIN-TREE-COMMIT-AMBIGUOUS {lane}: {} could be any of {}",
            commit.sha,
            commit.lanes.join(" ")
        );
    }
}

/// Poll `lane_result_rc` every `interval` until a result appears or `deadline`
/// passes. `None` on deadline is a timeout; `since` bounds satisfying rows.
#[cfg(test)]
fn wait_for_result(
    dir: &std::path::Path,
    lane: &str,
    deadline: Option<std::time::Duration>,
    interval: std::time::Duration,
) -> Option<i32> {
    match wait_for_outcome(dir, lane, deadline, interval, &|_, _| {
        RouteLiveness::Unknown
    }) {
        WaitOutcome::Result(rc) => Some(rc),
        WaitOutcome::Died | WaitOutcome::TimedOut => None,
    }
}

/// As `wait_for_result`, plus the liveness probe that turns a vanished lane
/// into `Died` instead of a wait that outlives the process it waits on.
fn wait_for_outcome(
    dir: &std::path::Path,
    lane: &str,
    deadline: Option<std::time::Duration>,
    interval: std::time::Duration,
    liveness: &dyn Fn(&std::path::Path, &str) -> RouteLiveness,
) -> WaitOutcome {
    let since = route_registered_at(dir, lane);
    let start = std::time::Instant::now();
    let mut dead_polls = 0u32;
    loop {
        if let Some(rc) = lane_result_rc_since(dir, lane, since) {
            return WaitOutcome::Result(rc);
        }
        dead_polls = match liveness(dir, lane) {
            RouteLiveness::Dead => dead_polls + 1,
            RouteLiveness::Live | RouteLiveness::Unknown => 0,
        };
        if dead_polls >= DEAD_POLLS {
            return WaitOutcome::Died;
        }
        if deadline.is_some_and(|limit| start.elapsed() >= limit) {
            return WaitOutcome::TimedOut;
        }
        std::thread::sleep(interval);
    }
}

/// `beep ps`, optionally narrowed to one lane.
fn run_ps(mail_dir_arg: Option<&Path>, lane: Option<&str>, all: bool) -> Result<()> {
    let snapshot = proc::SysinfoSnapshot::capture()?;
    run_ps_with(mail_dir_arg, lane, all, &snapshot)
}

/// Takes the `ProcReader` seam rather than the concrete snapshot, so a fake
/// reader can drive this without a real process tree.
fn run_ps_with(
    mail_dir_arg: Option<&Path>,
    lane: Option<&str>,
    all: bool,
    reader: &dyn proc::ProcReader,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    line("lane\tpid\trss_kb\tcpu_pct\tuptime_sec\tchildren");
    for (name, route) in &routes {
        if let Some(want) = lane {
            if name != want {
                continue;
            }
        }
        let pane_pid = route
            .tmux
            .as_deref()
            .and_then(|target| tmux::mux().pane_pid(None, target))
            .unwrap_or(0);
        match proc::tree_sum_of(reader, pane_pid) {
            Some(sum) => {
                let now = now_unix_secs();
                let uptime = proc::uptime_secs(sum.start_time_secs, now);
                println!(
                    "{}\t{}\t{}\t{:.1}\t{}\t{}",
                    name,
                    pane_pid,
                    sum.rss_bytes / 1024,
                    sum.cpu_percent,
                    uptime,
                    reader.descendant_count(pane_pid),
                );
            }
            // A dead route prints only when asked for by name or --all.
            None if all || lane.is_some() => {
                println!("{}\t{}\t-\t-\t-\t-", name, pane_pid);
            }
            None => {}
        }
    }
    Ok(())
}

fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// pstree
// ---------------------------------------------------------------------------

/// The resolved `from -> to` summon edge for a lane. Explicit beats inferred.
#[derive(Clone, Debug)]
struct LaneEdge {
    /// The summoning lane, `None` for a true root.
    parent: Option<String>,
    /// `true` when the edge came from the first dispatch row, not a route
    /// `--parent`.
    inferred: bool,
}

fn resolve_edges(
    routes: &BTreeMap<String, Route>,
    messages: &[bus::Message],
) -> BTreeMap<String, LaneEdge> {
    routes
        .iter()
        .map(|(name, route)| {
            let edge = match &route.parent {
                Some(parent) => LaneEdge {
                    parent: Some(parent.clone()),
                    inferred: false,
                },
                None => {
                    let summoner = messages
                        .iter()
                        .find(|message| message.kind == "dispatch" && message.to == *name)
                        .and_then(|message| {
                            (!message.from.is_empty()).then(|| message.from.clone())
                        });
                    match summoner {
                        Some(parent) => LaneEdge {
                            parent: Some(parent),
                            inferred: true,
                        },
                        None => LaneEdge {
                            parent: None,
                            inferred: false,
                        },
                    }
                }
            };
            (name.clone(), edge)
        })
        .collect()
}

struct LaneMeta {
    pid: u32,
    state: &'static str,
    descendants: Vec<ProcessDesc>,
}

#[derive(Clone)]
struct ProcessDesc {
    pid: u32,
    comm: String,
}

/// One renderable node: a real lane or a `[gone]` phantom for a summoner that
/// is not itself a known lane.
struct LaneNode {
    name: String,
    parent: Option<String>,
    inferred: bool,
    pid: u32,
    state: &'static str,
    descendants: Vec<ProcessDesc>,
    goal: Option<String>,
    gone: bool,
    children: Vec<usize>,
}

fn build_lane_nodes(
    routes: &BTreeMap<String, Route>,
    edges: &BTreeMap<String, LaneEdge>,
    meta: &BTreeMap<String, LaneMeta>,
    include: &BTreeSet<String>,
) -> Vec<LaneNode> {
    let mut nodes: Vec<LaneNode> = Vec::new();
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    for name in include {
        let lane = meta.get(name).expect("included lane has meta");
        let edge = edges.get(name).expect("included lane has edge");
        let idx = nodes.len();
        nodes.push(LaneNode {
            name: name.clone(),
            parent: edge.parent.clone(),
            inferred: edge.inferred,
            pid: lane.pid,
            state: lane.state,
            descendants: lane.descendants.clone(),
            goal: routes.get(name).and_then(|route| route.goal.clone()),
            gone: false,
            children: Vec::new(),
        });
        index.insert(name.clone(), idx);
    }
    let mut phantom: BTreeSet<String> = BTreeSet::new();
    for name in include {
        if let Some(parent) = edges.get(name).and_then(|edge| edge.parent.as_deref()) {
            if !include.contains(parent) {
                phantom.insert(parent.to_owned());
            }
        }
    }
    for name in phantom {
        let idx = nodes.len();
        nodes.push(LaneNode {
            name: name.clone(),
            parent: None,
            inferred: false,
            pid: 0,
            state: "gone",
            descendants: Vec::new(),
            goal: None,
            gone: true,
            children: Vec::new(),
        });
        index.insert(name, idx);
    }
    for idx in 0..nodes.len() {
        let parent = nodes[idx].parent.clone();
        if let Some(parent) = parent {
            if let Some(&parent_idx) = index.get(&parent) {
                nodes[parent_idx].children.push(idx);
            }
        }
    }
    let names: Vec<String> = nodes.iter().map(|node| node.name.clone()).collect();
    for node in &mut nodes {
        node.children.sort_by_key(|&child| names[child].clone());
    }
    nodes
}

fn run_pstree(mail_dir_arg: Option<&Path>, all: bool, format: PstreeFormat) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let messages = all_messages(&dir)?;
    let edges = resolve_edges(&routes, &messages);
    let snapshot = proc::SysinfoSnapshot::capture()?;
    let mut meta: BTreeMap<String, LaneMeta> = BTreeMap::new();
    let mut include: BTreeSet<String> = BTreeSet::new();
    for (name, route) in &routes {
        let pane_pid = route
            .tmux
            .as_deref()
            .and_then(|target| tmux::mux().pane_pid(None, target))
            .unwrap_or(0);
        let live = snapshot.process(pane_pid).is_some();
        if !all && !live {
            continue;
        }
        include.insert(name.clone());
        let descendants = snapshot
            .descendants(pane_pid)
            .into_iter()
            .filter_map(|pid| {
                snapshot.process(pid).map(|info| ProcessDesc {
                    pid,
                    comm: info.name,
                })
            })
            .collect();
        meta.insert(
            name.clone(),
            LaneMeta {
                pid: pane_pid,
                state: if live { "live" } else { "dead" },
                descendants,
            },
        );
    }
    let nodes = build_lane_nodes(&routes, &edges, &meta, &include);
    match format {
        PstreeFormat::Text => {
            for output in render_text(&nodes) {
                line(&output);
            }
        }
        PstreeFormat::Ndjson => {
            for output in render_ndjson(&nodes) {
                line(&output);
            }
        }
    }
    Ok(())
}

fn render_text(nodes: &[LaneNode]) -> Vec<String> {
    fn emit(out: &mut Vec<String>, nodes: &[LaneNode], idx: usize, depth: usize) {
        let node = &nodes[idx];
        out.push(format!(
            "{}{}",
            "  ".repeat(depth),
            match node.gone {
                true => format!("{} [gone]", node.name),
                false => {
                    let pid = if node.pid == 0 {
                        "-".to_owned()
                    } else {
                        node.pid.to_string()
                    };
                    format!(
                        "{} ({pid}) [{}]{}{}",
                        node.name,
                        node.state,
                        if node.inferred { " [inferred]" } else { "" },
                        match &node.goal {
                            Some(goal) => format!(" -- {goal}"),
                            None => String::new(),
                        }
                    )
                }
            }
        ));
        if !node.gone {
            for desc in &node.descendants {
                out.push(format!(
                    "{}  {} ({})",
                    "  ".repeat(depth + 1),
                    desc.comm,
                    desc.pid
                ));
            }
        }
        for child in &node.children {
            emit(out, nodes, *child, depth + 1);
        }
    }
    let roots: Vec<usize> = nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.parent.is_none())
        .map(|(idx, _)| idx)
        .collect();
    let mut out = Vec::new();
    for root in roots {
        emit(&mut out, nodes, root, 0);
    }
    out
}

fn render_ndjson(nodes: &[LaneNode]) -> Vec<String> {
    nodes
        .iter()
        .map(|node| {
            serde_json::json!({
                "lane": node.name,
                "parent": node.parent,
                "inferred": node.inferred,
                "pid": if node.gone { None } else { Some(node.pid) },
                "state": node.state,
                "goal": node.goal,
                "children": node.descendants.iter().map(|desc| desc.pid).collect::<Vec<_>>(),
            })
            .to_string()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// whoami
// ---------------------------------------------------------------------------

fn run_me_favorite(index: i64, note: Option<&str>) -> Result<()> {
    anyhow::ensure!(
        index < 0,
        "favorite index must be negative; -1 is the newest assistant message"
    );

    let dir = mail_dir(None)?;
    let routes = bus::read_routes(&dir).unwrap_or_default();
    let identity = identity::resolve(&routes)?;
    let session = identity
        .session
        .context("no caller session resolved; run `boop me` once in this tmux pane, then retry")?;

    let store = open_store()?;
    let rows = store.turn_rows(&ident::TurnQuery {
        session: Some(session.clone()),
        role: Some("assistant".to_owned()),
        ..Default::default()
    })?;
    let offset = index
        .checked_neg()
        .and_then(|value| value.checked_sub(1))
        .context("favorite index is outside the supported range")? as usize;
    let row = rows.iter().rev().nth(offset).with_context(|| {
        format!(
            "session {session} has {} assistant messages; cannot select {index}",
            rows.len()
        )
    })?;
    anyhow::ensure!(
        !row.said.trim().is_empty(),
        "selected assistant message is empty"
    );
    let source = format!("{}:{}:assistant:{}", row.harness, session, row.turn);
    let id = store.favorite_add(&row.said, note.unwrap_or(""), &source, now_ms())?;
    line(&format!("favorite {id}"));
    Ok(())
}

fn run_me(name: Option<&str>, mail_dir_arg: Option<&Path>) -> Result<()> {
    let pane = std::env::var("TMUX_PANE")
        .ok()
        .filter(|pane| !pane.is_empty())
        .or_else(|| tmux::mux().current_pane(None))
        .context("resolve current tmux pane; run boop me inside tmux")?;
    let cwd = std::env::current_dir().context("read current directory")?;
    let session = boop::harness::codex::latest_root_session_for_cwd(&cwd)?
        .context("no root Codex transcript records the current directory")?;
    let generated = format!("codex-{}", pane.trim_start_matches('%'));
    let name = name.unwrap_or(&generated);
    let dir = mail_dir(mail_dir_arg)?;
    write_route(
        &dir,
        name,
        Route {
            kind: "coordinator".into(),
            harness: Some("codex".into()),
            tmux: Some(pane.clone()),
            cwd: Some(cwd.display().to_string()),
            model: None,
            mode: Some("interactive".into()),
            session_id: Some(session.session_id.clone()),
            source_path: Some(session.path.display().to_string()),
            parent: None,
            goal: None,
            registered_at: Some(bus::now_iso()),
            base_sha: None,
            worktree_dir: None,
        },
    )?;
    println!("registered {name} -> {pane} codex {}", session.session_id);
    if let Ok(mood) = boop::Store::default_path()
        .and_then(boop::Store::open)
        .and_then(|store| store.effective_mood(name))
    {
        println!("{}", mood.line());
    }
    Ok(())
}

/// Read or write the caller's mood. Writing validates the name against the
/// stored moods, so a typo never reaches a delivery path.
fn run_me_mood(
    mood: Option<&str>,
    clear: bool,
    as_name: Option<&str>,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let session = waiting_as(&dir, as_name)?;
    let store = boop::Store::open(boop::Store::default_path()?)?;
    match (mood, clear) {
        (Some(mood), _) => {
            store.set_session_mood(&session, mood, boop::channel::now_ms())?;
            println!("mood: {mood} (set on {session})");
        }
        (None, true) => {
            let had = store.clear_session_attr(&session, boop::ident::MOOD_ATTR_KEY)?;
            match had {
                true => println!("mood cleared on {session}"),
                false => println!("{session} had no mood of its own"),
            }
            println!("{}", store.effective_mood(&session)?.line());
        }
        (None, false) => println!("{}", store.effective_mood(&session)?.line()),
    }
    Ok(())
}

fn run_whoami(json: bool) -> Result<()> {
    let dir = mail_dir(None)?;
    let routes = bus::read_routes(&dir).unwrap_or_default();
    let identity = identity::resolve(&routes)?;
    if json {
        println!("{}", identity.to_json());
        return Ok(());
    }
    let rung = identity.rung.unwrap_or(identity::Rung::None);
    println!("session  {}", identity.session.as_deref().unwrap_or("-"));
    println!("lane     {}", identity.lane.as_deref().unwrap_or("-"));
    println!("parent   {}", identity.parent.as_deref().unwrap_or("-"));
    println!("harness  {}", identity.harness.as_deref().unwrap_or("-"));
    println!("pane     {}", identity.pane.as_deref().unwrap_or("-"));
    println!("rung     {} ({})", rung.as_str(), rung.confidence());
    Ok(())
}

/// An opencode route handed to `codex exec -m` is a broken invocation, so a
/// default preset whose model routes elsewhere goes unused.
fn default_preset_for_harness(
    config: &config::Config,
    config_path: &Path,
    harness_id: &str,
) -> Result<Option<String>> {
    let Some(preset) = config.default_model_preset.as_deref() else {
        return Ok(None);
    };
    let model = config::resolve_model(preset, config_path)?;
    match lane::harness_for_model(&model)? {
        Some(owner) if owner == harness_id => Ok(Some(preset.to_owned())),
        _ => Ok(None),
    }
}

/// `boop config path` prints the resolved config path; `boop config show`
/// prints the loaded config as pretty JSON, including the defaults a missing
/// file produces.
fn run_config(cmd: ConfigCmd) -> Result<()> {
    match cmd {
        ConfigCmd::Path => line(&config::default_path()?.display().to_string()),
        ConfigCmd::Show => line(&config::show(&config::default_path()?)?),
        ConfigCmd::Presets => line(&presets_table()?),
    }
    Ok(())
}

/// Each preset resolved to model, variant, and the harness the model spelling
/// names, with the `default-model-preset` row marked.
fn presets_table() -> Result<String> {
    let path = config::default_path()?;
    let config = config::load(&path)?;
    let mut rows: Vec<[String; 5]> = vec![[
        "PRESET".into(),
        "MODEL".into(),
        "VARIANT".into(),
        "HARNESS".into(),
        "DEFAULT".into(),
    ]];
    for name in config.model_presets.keys() {
        let preset = config::resolve_preset(name, &path)?;
        let harness = lane::harness_for_model(&preset.model)?
            .map(|harness| harness.into_owned())
            .unwrap_or_else(|| "?".to_owned());
        let default = if config.default_model_preset.as_deref() == Some(name) {
            "*"
        } else {
            ""
        };
        rows.push([
            name.clone(),
            preset.model,
            preset.variant.unwrap_or_default(),
            harness,
            default.to_owned(),
        ]);
    }
    if rows.len() == 1 {
        return Ok(format!("no model presets in {}", path.display()));
    }
    let mut widths = [0usize; 5];
    for row in &rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.len());
        }
    }
    let table = rows
        .iter()
        .map(|row| {
            row.iter()
                .zip(widths)
                .map(|(cell, width)| format!("{cell:<width$}"))
                .collect::<Vec<_>>()
                .join("  ")
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(table)
}
