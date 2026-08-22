//! `boop`: the cross-harness agent-event reader, 1-1 with `bus` plus the four
//! verbs `bus` cannot do (read what an agent did, and measure what its
//! processes cost). The CLI routes to layers 0-3; it contains no `match` on
//! harness id and no direct `Command::new("tmux")` beyond the layer-1 helpers.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use tracing_subscriber::EnvFilter;

use boop::registry::Registry;
use boop::supervise::ParentDeathPolicy;
use boop::{bus, config, identity, mailwait};

mod cli;

use cli::control::run_native_tui;
use cli::db::{
    run_chat_query, run_db, run_follow, run_harnesses, run_passthrough, run_public_agent_command,
    run_query, run_sessions, run_sync_all, run_tail, sync_all, ChatQueryOptions, SyncLiveness,
};
use cli::debug::{run_config, run_debug, run_host};
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
    /// Run a persistent foreground ACP coordinator. Agent names such as
    /// `codex` work directly; model preset names resolve through config.
    #[arg(long)]
    preset: Option<String>,
    /// Registry and ACPX session name for the foreground coordinator.
    #[arg(long, requires = "preset")]
    name: Option<String>,
    /// Mail registry for the foreground coordinator.
    #[arg(long, requires = "preset")]
    mail_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<SubCmd>,
}

#[derive(Subcommand)]
enum SubCmd {
    /// Print shell functions that route interactive harnesses through Boop.
    ShellInit {
        #[arg(value_enum)]
        shell: ShellKind,
    },
    /// Launch an ordinary interactive harness TUI and register this pane.
    Tui {
        /// Registered harness adapter: claude, codex, kimi, or opencode.
        harness: String,
        /// Executable override, for example ccz with the Claude adapter.
        #[arg(long = "bin")]
        executable: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
        /// Arguments forwarded to the ordinary harness TUI.
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Launch a native Codex TUI attached to a Boop-owned managed app-server.
    Codex {
        /// Registry name. Defaults to the current tmux pane's Codex identity.
        #[arg(long)]
        name: Option<String>,
        /// Working directory for the native TUI.
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
        /// Arguments forwarded to the ordinary Codex TUI.
        #[arg(last = true)]
        args: Vec<String>,
    },
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
        #[arg(long)]
        mail_dir: Option<PathBuf>,
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

#[derive(Clone, Copy, ValueEnum)]
enum ShellKind {
    Bash,
}

const BASH_SHELL_INIT: &str = r#"codex() {
  command boop tui codex --cwd "$PWD" -- "$@"
}

claude() {
  command boop tui claude --cwd "$PWD" -- "$@"
}

ccz() {
  command boop tui claude --bin ccz --cwd "$PWD" -- "$@"
}

kimi() {
  command boop tui kimi --cwd "$PWD" -- "$@"
}

opencode() {
  command boop tui opencode --cwd "$PWD" -- "$@"
}"#;

fn print_shell_init(shell: ShellKind) {
    match shell {
        ShellKind::Bash => println!("{BASH_SHELL_INIT}"),
    }
}

/// Whether this invocation is asking for help, whatever verb it names.
fn help_wanted() -> bool {
    std::env::args().any(|argument| argument == "--help" || argument == "-h")
}

fn main() -> Result<()> {
    // Only a help invocation pays for the trail read the banner needs.
    if help_wanted() {
        if let Some(banner) = boop::debug::help_banner(now_ms()) {
            line(&banner);
        }
    }
    let cli = Cli::parse();
    if let Some(preset) = cli.preset.as_deref() {
        anyhow::ensure!(
            cli.command.is_none(),
            "--preset cannot be combined with a subcommand"
        );
        return cli::acpx::run_foreground(preset, cli.name.as_deref(), cli.mail_dir.as_deref());
    }
    let command = cli.command.context("a command or --preset is required")?;
    init_tracing(supervised_lane(&command))?;
    let registry = Registry::discover();
    let needs_startup_sync = command_needs_startup_sync(&command);
    run_with_startup_sync(
        needs_startup_sync,
        || sync_before_local_command(&registry),
        || match command {
            SubCmd::ShellInit { shell } => {
                print_shell_init(shell);
                Ok(())
            }
            SubCmd::Codex {
                name,
                cwd,
                mail_dir,
                args,
            } => {
                let cwd = cwd.unwrap_or(std::env::current_dir().context("read current directory")?);
                let adapter = registry.get(boop::harness::HarnessId::Codex);
                run_native_tui(
                    &registry,
                    adapter,
                    name.as_deref(),
                    &cwd,
                    mail_dir.as_deref(),
                    None,
                    &args,
                )
            }
            SubCmd::Tui {
                harness,
                executable,
                name,
                cwd,
                mail_dir,
                args,
            } => {
                let cwd = cwd.unwrap_or(std::env::current_dir()?);
                let adapter = registry
                    .by_name(&harness)
                    .with_context(|| format!("no harness registered with id `{harness}`"))?;
                run_native_tui(
                    &registry,
                    adapter,
                    name.as_deref(),
                    &cwd,
                    mail_dir.as_deref(),
                    executable.as_deref(),
                    &args,
                )
            }
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
            SubCmd::Whoami { json, mail_dir } => run_whoami(json, mail_dir.as_deref()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser, Subcommand};

    use boop::ident;

    #[test]
    fn public_agent_summary_command_parses() {
        let cli = Cli::try_parse_from(["boop", "agent", "summary", "--format", "text"])
            .expect("public agent summary command parses");
        assert!(matches!(
            cli.command,
            Some(SubCmd::Agent {
                cmd: AgentSummaryCmd::Summary { .. }
            })
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
                !command_needs_startup_sync(cli.command.as_ref().unwrap()),
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
                command_needs_startup_sync(cli.command.as_ref().unwrap()),
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

    #[test]
    fn foreground_acp_coordinator_parses_without_a_subcommand() {
        let cli = Cli::try_parse_from(["boop", "--preset", "codex", "--name", "root"]).unwrap();
        assert_eq!(cli.preset.as_deref(), Some("codex"));
        assert_eq!(cli.name.as_deref(), Some("root"));
        assert!(cli.mail_dir.is_none());
        assert!(cli.command.is_none());
    }
}
