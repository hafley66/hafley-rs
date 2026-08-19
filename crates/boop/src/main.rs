//! `boop`: the cross-harness agent-event reader, 1-1 with `bus` plus the four
//! verbs `bus` cannot do (read what an agent did, and measure what its
//! processes cost). The CLI routes to layers 0-3; it contains no `match` on
//! harness id and no direct `Command::new("tmux")` beyond the layer-1 helpers.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use tracing::warn;
use tracing_subscriber::EnvFilter;

use boop::registry::Registry;
use boop::supervise::ParentDeathPolicy;
use boop::{bus, config, identity, lane, mailwait, proc};

mod cli;

use cli::db::{
    open_ro_store, run_chat_query, run_db, run_follow, run_harnesses, run_passthrough,
    run_public_agent_command, run_query, run_sessions, run_sync_all, run_tail, sync_all,
    ChatQueryOptions, SyncLiveness,
};
use cli::job::{
    run_beep, run_dispatch, run_lane, run_measure, run_resolve, run_sweep, run_wait, DispatchArgs,
    LaneArgs,
};
use cli::mail::{run_hail, run_inbox, run_list, run_tell_children, run_tell_parent};
use cli::me::{run_adopt, run_me, run_me_favorite, run_me_mood, run_prune, run_whoami, HookWiring};
use cli::{doctrine, line, mail_dir, now_ms, CONCATMAP_EXAMPLES};

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

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser, Subcommand};
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
