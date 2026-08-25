//! `boop`: the cross-harness agent-event reader, 1-1 with `bus` plus the four
//! verbs `bus` cannot do (read what an agent did, and measure what its
//! processes cost). The CLI routes to layers 0-3; it contains no `match` on
//! harness id and no direct `Command::new("tmux")` beyond the layer-1 helpers.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};

use boop::registry::Registry;
use boop::supervise::ParentDeathPolicy;
use boop::{bus, mailwait};
#[cfg(feature = "dl6")]
use boop::{config, identity};

mod cli;

use cli::control::run_native_tui;
use cli::db::{
    run_chat_query, run_db, run_follow, run_harnesses, run_passthrough, run_public_agent_command,
    run_query, run_sessions, run_sync_all, run_tail, sync_before_read, ChatQueryOptions,
};
#[cfg(feature = "dl6")]
use cli::debug::run_host;
use cli::debug::{run_config, run_debug, run_lane_debug};
use cli::job::{
    run_beep, run_dispatch, run_lane, run_lane_wait, run_measure, run_resolve, run_sweep, run_wait,
    DispatchArgs, LaneArgs,
};
use cli::mail::{
    run_hail, run_inbox, run_list, run_push, run_send, run_tell_children, run_tell_parent, Outbound,
};
use cli::me::{run_adopt, run_me, run_me_favorite, run_me_mood, run_prune, run_whoami};
#[cfg(feature = "dl6")]
use cli::CONCATMAP_EXAMPLES;
use cli::{doctrine, line, mail_dir, now_ms};

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
    /// Folded (one-pane-register-path): `boop tui <harness>` is the spelling.
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
    /// Folded (one-pane-register-path): `boop tui codex` is the spelling.
    #[command(hide = true)]
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
    /// Mail a route and block for its answer; also the group that drives
    /// harnesses, lanes, agents and processes.
    ///
    /// `boop beep <route> <body>` is the one send. `<route>` is a lane, a
    /// coordinator, a native, `parent` (the caller's own parent edge) or
    /// `children` (every live child of the caller).
    #[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
    Beep {
        /// A registry name, or the `parent` / `children` alias.
        ///
        /// Clap cannot mark this required: `subcommand_negates_reqs` does not
        /// clear a parent positional once a `beep` subcommand matches, so
        /// `beep lane list` would fail on a missing `<ROUTE>`. The dispatch
        /// raises clap's own missing-argument error instead.
        #[arg(value_name = "ROUTE")]
        route: Option<String>,
        /// The message.
        #[arg(value_name = "BODY")]
        body: Option<String>,
        /// The older spelling of the BODY positional.
        #[arg(long = "body", hide = true)]
        body_flag: Option<String>,
        /// Who the row is from, when the whoami ladder cannot say.
        #[arg(long = "as", value_name = "NAME")]
        as_name: Option<String>,
        /// The mail kind the row wears.
        #[arg(long, default_value = "request")]
        kind: String,
        /// Seconds to block before exiting 124.
        #[arg(long, default_value_t = mailwait::DEFAULT_TIMEOUT_SECS)]
        timeout: u64,
        /// Send and return, instead of blocking for the answer.
        #[arg(long)]
        no_wait: bool,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
        #[command(subcommand)]
        cmd: Option<BeepCmd>,
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
        /// One lane, answered in full: route, mail, worktree, transcript,
        /// alerts. Without it, the WARN/ERROR window across every lane.
        #[arg(value_name = "LANE")]
        lane_arg: Option<String>,
        /// Window to read back, as `Ns`, `Nm`, `Nh` or a count of seconds.
        #[arg(long, default_value = "2m")]
        since: String,
        /// One lane only, for the alert window.
        #[arg(long)]
        lane: Option<String>,
        /// One JSON document, `alerts` and `sync`, instead of the grouped text.
        #[arg(long)]
        json: bool,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
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
    /// Folded (comment-out-dl6-verbs): the DL6 runtime, off by default;
    /// `cargo build --features dl6` puts it back.
    #[cfg(feature = "dl6")]
    #[command(hide = true)]
    Concatmap {
        /// The directory holding the mailbox `--me` resolves the caller in.
        #[arg(long = "mail-dir")]
        mail_dir_arg: Option<PathBuf>,
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
    /// Folded (comment-out-dl6-verbs): DL6, off by default.
    #[cfg(feature = "dl6")]
    #[command(hide = true)]
    Host {
        #[command(subcommand)]
        cmd: HostCmd,
    },
    /// Folded (2026-08-25): `boop beep parent <body>` is the spelling; this
    /// one is a hidden alias over the same send.
    #[command(hide = true)]
    TellParent {
        /// What the row says it is. `yield` carries a default body.
        #[arg(long, default_value = "note", value_parser = ["completion", "yield", "note"])]
        kind: String,
        /// The message. Required for every kind but `yield`.
        #[arg(long)]
        body: Option<String>,
        /// Who is calling, when the whoami ladder cannot say. A native subagent
        /// inherits its spawner's `BOOP_LANE`, so it names itself here.
        #[arg(long = "as", value_name = "NAME")]
        as_name: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Folded (2026-08-25): `boop beep children <body>` is the spelling; this
    /// one is a hidden alias over the same send.
    #[command(hide = true)]
    TellChildren {
        /// The message every child gets.
        #[arg(long)]
        body: String,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Report the caller's own identity and which of the two rungs named it.
    Whoami {
        #[arg(long)]
        json: bool,
        /// Who is calling. The first rung; `--from` is the same flag.
        #[arg(long = "as", alias = "from", value_name = "NAME")]
        as_name: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Block until mail lands: the reply to <id>, a lane's result row, or
    /// the next unread row addressed to you with --me.
    Wait {
        /// A message id, or a registered lane's name. Omit and pass --me.
        #[arg(value_name = "ID-OR-LANE", required_unless_present = "me")]
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
    /// Folded (2026-08-25): `boop beep <route> <body>` is the spelling; this
    /// one is a hidden alias over the same send.
    #[command(hide = true)]
    Push {
        /// The route to push at: a lane, a coordinator, or `parent`.
        #[arg(value_name = "ROUTE")]
        to: String,
        #[arg(long)]
        body: String,
        /// Seconds to block before exiting 124.
        #[arg(long, default_value_t = mailwait::DEFAULT_TIMEOUT_SECS)]
        timeout: u64,
        /// The mail kind the row wears.
        #[arg(long, default_value = "request")]
        kind: String,
        /// Who the row is from, when no env stamp says. `--from` is the same
        /// flag, kept for briefs that spell it that way.
        #[arg(long = "as", alias = "from", value_name = "NAME")]
        from: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Mail a claude coordinator reads at a turn boundary: the hook inbox.
    /// Folded (door-only-claude-delivery): the hook inbox is a rung the
    /// delivery ladder walks on its own, not a verb a caller reaches for. The
    /// installed hook still calls `boop inbox drain`, so the group runs.
    #[command(hide = true)]
    Inbox {
        #[command(subcommand)]
        cmd: InboxCmd,
    },
    /// Register this Codex pane, or act on the caller's own conversation.
    /// Folded (one-pane-register-path): `boop beep agent register` registers;
    /// `boop me mood` / `boop me favorite` still run under this name.
    #[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
    #[command(hide = true)]
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
        /// The route to hail, or `parent` for the caller's own parent edge.
        #[arg(long)]
        to: String,
        #[arg(long)]
        body: String,
        /// Who the row is from; `--from` is the same flag.
        #[arg(long = "as", alias = "from", value_name = "NAME")]
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
    /// spawns. Mail to the adopted route goes through its harness's own door;
    /// `boop inbox hooks` is the fallback for a session with no live door.
    #[command(hide = true)]
    Adopt {
        #[arg(long)]
        name: String,
        #[arg(long)]
        tmux: String,
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
    let needs_startup_sync = startup_sync_wanted(&command, sync_suppressed());
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
            SubCmd::Sync { rebuild } => run_sync_all(&registry, rebuild, None),
            SubCmd::Debug {
                lane_arg,
                since,
                lane,
                json,
                mail_dir,
            } => match lane_arg.as_deref().or(lane.as_deref()).filter(|_| !json) {
                Some(lane) => run_lane_debug(lane, &since, mail_dir.as_deref()),
                None => run_debug(&since, lane.as_deref(), json),
            },
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
                    effort: None,
                    variant: None,
                    bin: None,
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
                    bin: None,
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
                uninstall_hooks,
            ),
            SubCmd::Prune { mail_dir } => run_prune(mail_dir.as_deref()),
            SubCmd::Beep {
                route,
                body,
                body_flag,
                as_name,
                kind,
                timeout,
                no_wait,
                mail_dir,
                cmd,
            } => match cmd {
                Some(cmd) => run_beep(&registry, cmd),
                None => run_send(
                    &registry,
                    Outbound {
                        route: &beep_route(route.as_deref()),
                        body: beep_body(body.as_deref().or(body_flag.as_deref())),
                        kind: &kind,
                        as_name: as_name.as_deref(),
                        box_name: None,
                        timeout_secs: timeout,
                        wait: !no_wait,
                        mail_dir: mail_dir.as_deref(),
                    },
                ),
            },
            SubCmd::Db { sql, format, cmd } => match cmd {
                Some(cmd) => run_db(&registry, cmd),
                None => match sql {
                    Some(sql) => run_passthrough(&sql, format.unwrap_or_default()),
                    None => anyhow::bail!(
                        "boop db needs a SQL string or a subcommand; see `boop db --help`"
                    ),
                },
            },
            #[cfg(feature = "dl6")]
            SubCmd::Concatmap {
                mail_dir_arg,
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
                // Explicit model wins, preset resolves through config.
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
                        let routes = bus::read_routes(&mail_dir(mail_dir_arg.as_deref())?)
                            .unwrap_or_default();
                        let identity = identity::resolve_with(&registry, &routes)?;
                        Some(identity.session.context(
                        "--me found no caller session: this process carries no BOOP_SESSION stamp; pass --session <id>",
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
            #[cfg(feature = "dl6")]
            SubCmd::Host { cmd } => run_host(cmd),
            SubCmd::Push {
                to,
                body,
                timeout,
                kind,
                from,
                mail_dir,
            } => run_push(
                &registry,
                &to,
                &body,
                &kind,
                from.as_deref(),
                timeout,
                mail_dir.as_deref(),
            ),
            SubCmd::Wait {
                id,
                me,
                as_name,
                wait_timeout,
                mail_dir,
            } => match id.as_deref() {
                Some(id) if wait_target_is_a_lane(mail_dir.as_deref(), id) => {
                    run_lane_wait(mail_dir.as_deref(), id, wait_timeout)
                }
                _ => run_wait(
                    id.as_deref(),
                    me,
                    as_name.as_deref(),
                    wait_timeout,
                    mail_dir.as_deref(),
                ),
            },
            SubCmd::TellParent {
                kind,
                body,
                as_name,
                mail_dir,
            } => run_tell_parent(
                &registry,
                &kind,
                body.as_deref(),
                as_name.as_deref(),
                mail_dir.as_deref(),
            ),
            SubCmd::TellChildren { body, mail_dir } => {
                run_tell_children(&registry, &body, mail_dir.as_deref())
            }
            SubCmd::Whoami {
                json,
                as_name,
                mail_dir,
            } => run_whoami(json, as_name.as_deref(), mail_dir.as_deref()),
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
            SubCmd::Config { cmd } => run_config(&registry, cmd),
        },
    )
}

/// The two positionals `boop beep <route> <body>` needs, raised as clap's own
/// missing-argument error when the caller left one out. Clap cannot mark them
/// required itself without breaking `boop beep lane list`, so the check runs
/// here and the reader still sees `<ROUTE>` / `<BODY>` and exit 2.
fn beep_route(route: Option<&str>) -> String {
    match route {
        Some(route) => route.to_owned(),
        None => missing_beep_argument("<ROUTE>"),
    }
}

fn beep_body(body: Option<&str>) -> Option<&str> {
    match body {
        Some(body) => Some(body),
        None => missing_beep_argument("<BODY>"),
    }
}

fn missing_beep_argument(name: &str) -> ! {
    Cli::command()
        .error(
            clap::error::ErrorKind::MissingRequiredArgument,
            format!("the following required arguments were not provided:\n  {name}\n\nUsage: boop beep <ROUTE> <BODY>\n"),
        )
        .exit()
}

fn sync_before_local_command(registry: &Registry) -> Result<()> {
    sync_before_read(registry)
}

/// Whether `boop wait <id>` names a registered lane, so it dispatches to
/// `run_lane_wait`. An unreadable registry falls through to the mail wait.
fn wait_target_is_a_lane(mail_dir_arg: Option<&std::path::Path>, id: &str) -> bool {
    let Ok(dir) = mail_dir(mail_dir_arg) else {
        return false;
    };
    bus::read_routes(&dir)
        .ok()
        .and_then(|routes| routes.get(id).map(|route| route.kind == "lane"))
        .unwrap_or(false)
}

/// The name of the escape hatch that suppresses the startup transcript sync.
const NO_SYNC_ENV: &str = "BOOP_NO_SYNC";

/// Whether the caller suppressed the startup sync. `instant`, hooks and shell
/// loops spawn boop as a subprocess and cannot thread a clap flag through, so
/// the hatch is an environment variable; `0` and the empty value keep syncing.
fn sync_suppressed() -> bool {
    match std::env::var(NO_SYNC_ENV) {
        Ok(value) => !value.is_empty() && value != "0",
        Err(_) => false,
    }
}

/// The startup-sync decision: the verb's own need, minus the caller's hatch.
/// The hatch never widens the set of verbs that sync.
fn startup_sync_wanted(command: &SubCmd, suppressed: bool) -> bool {
    !suppressed && command_needs_startup_sync(command)
}

/// Verbs that read `agent_*` rows. A registry, mailbox, tmux or live-process
/// verb stays off: a cold cursor re-parses every transcript root from offset 0.
fn command_needs_startup_sync(command: &SubCmd) -> bool {
    #[cfg(feature = "dl6")]
    if matches!(command, SubCmd::Concatmap { .. }) {
        return true;
    }
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
    let config = hafley_observe::Config::from_env("boop", boop::BUILD, "info", ansi)?;
    hafley_observe::init_with_writer(config, boop::trail::lane_writer(lane_log))
        .map_err(|error| anyhow::anyhow!("initialise tracing subscriber: {error}"))
}

/// The lane this invocation supervises, which is the only verb whose whole run
/// belongs in one lane's trail.
fn supervised_lane(command: &SubCmd) -> Option<&str> {
    match command {
        SubCmd::Beep {
            cmd: Some(BeepCmd::Lane {
                cmd: LaneCmd::Run { lane, .. },
            }),
            ..
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
    /// Folded (2026-08-25): `boop beep <route> <body> --no-wait` is the
    /// spelling; this one is a hidden alias over the same send.
    #[command(hide = true)]
    Hail {
        /// The route to hail: a lane, a coordinator, a native, or `parent`.
        lane: String,
        #[arg(long)]
        body: String,
        /// Who the row is from; `--from` is the same flag.
        #[arg(long = "as", alias = "from", value_name = "NAME")]
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
    /// Folded (audit 2026-08-25): its one verb, `ack`, is folded with it.
    #[command(hide = true)]
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
    /// Folded (audit 2026-08-25): `beep lane list` carries the parent column.
    #[command(hide = true)]
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
        /// Folded (presets-only-model-spelling): the preset table names the
        /// harness. Hidden alias, still honoured.
        #[arg(long, hide = true)]
        harness: Option<String>,
        /// Folded (presets-only-model-spelling): `--preset <name>` is the
        /// spelling. Hidden alias, still honoured.
        #[arg(long, hide = true)]
        model: Option<String>,
        /// The row of the config preset table this lane spawns from: harness,
        /// model, effort. The one model spelling `lane create` takes.
        #[arg(long, conflicts_with = "model")]
        preset: Option<String>,
        /// opencode reasoning-effort variant (low|medium|high); CLI wins over
        /// the preset's variant, and opencode's default applies when neither.
        #[arg(long)]
        variant: Option<String>,
        /// Run the harness as this executable instead of its own binary
        /// (`ccz` is claude under the z.ai env). CLI wins over the preset's
        /// `bin`; the harness's own binary applies when neither names one.
        #[arg(long)]
        bin: Option<String>,
        /// Block until the lane's on-exit result row lands, then exit with its
        /// rc. Without a parent, the waiter owns a private result recipient.
        /// Folded (one-wait-verb): `boop wait <lane>` is the spelling now.
        #[arg(long, hide = true)]
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
    /// Folded (audit 2026-08-25): the supervisor's entry point, spawned by `lane create`.
    #[command(hide = true)]
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
        /// Reasoning effort, threaded from the preset; the harness spells it
        /// as its own config, never as `model@effort`.
        #[arg(long)]
        effort: Option<String>,
        /// Continue an existing harness conversation instead of opening one.
        #[arg(long)]
        resume: Option<String>,
        /// opencode reasoning-effort variant, threaded from `lane create`.
        #[arg(long)]
        variant: Option<String>,
        /// The executable the harness runs as, threaded from `lane create`.
        #[arg(long)]
        bin: Option<String>,
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
        /// One lane: kill its pane and drop its route. Omit for a bulk delete
        /// by `--state`.
        lane: Option<String>,
        /// Drop only the registry route; never kill the pane. The `--parent`
        /// on-exit epilogue uses this to clean up while still running inside it.
        #[arg(long)]
        route_only: bool,
        /// Bulk delete: `dead` removes every dead lane's route and its own
        /// worktree, and nothing above it. Pair with `--dry-run` first.
        #[arg(long)]
        state: Option<String>,
        /// Bulk delete only: print every route and every worktree path the
        /// delete would remove, and remove nothing.
        #[arg(long)]
        dry_run: bool,
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
    /// Folded (audit 2026-08-25): `beep lane get` prints the same route row.
    #[command(hide = true)]
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
    /// Folded (one-wait-verb): `boop wait <lane>` is the spelling now.
    #[command(hide = true)]
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
        /// The route name. Every boop call this agent makes then carries
        /// `--as <name>`: it shares its spawner's process, so no env stamp
        /// can name it.
        name: String,
        /// `native` (a subagent inside a lane or coordinator process) or
        /// `coordinator` (a pane-less session that owns lanes).
        #[arg(long, default_value = "native")]
        kind: String,
        /// The route completion and `boop beep parent` rows go to.
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
    /// Folded (audit 2026-08-25): age-based bulk-mark proves no read and no compliance.
    #[command(hide = true)]
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
    /// Folded (audit 2026-08-25): one-table dump; `boop db "SELECT * FROM agent_session"` answers it.
    #[command(hide = true)]
    Session {
        #[command(subcommand)]
        cmd: SessionCmd,
    },
    /// Rows from `agent_turn`: one row per user/assistant turn.
    /// Folded (audit 2026-08-25): one-table dump; `boop db "SELECT * FROM agent_turn"` answers it.
    #[command(hide = true)]
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
    /// Folded (audit 2026-08-25): one-table dump; `boop db "SELECT * FROM agent_touch"` answers it.
    #[command(hide = true)]
    Touch {
        #[command(subcommand)]
        cmd: FactCmd,
    },
    /// Rows from `agent_cmd`: shell commands a session ran.
    #[cfg(feature = "agent-read")]
    /// Folded (audit 2026-08-25): one-table dump; `boop db "SELECT * FROM agent_cmd"` answers it.
    #[command(hide = true)]
    Command {
        #[command(subcommand)]
        cmd: FactCmd,
    },
    /// Rows from `agent_fetch`: URLs a session fetched.
    #[cfg(feature = "agent-read")]
    /// Folded (audit 2026-08-25): one-table dump; `boop db "SELECT * FROM agent_fetch"` answers it.
    #[command(hide = true)]
    Fetch {
        #[command(subcommand)]
        cmd: FactCmd,
    },
    /// Rows from `agent_skill`: skills a session invoked.
    #[cfg(feature = "agent-read")]
    /// Folded (audit 2026-08-25): one-table dump; `boop db "SELECT * FROM agent_skill"` answers it.
    #[command(hide = true)]
    Skill {
        #[command(subcommand)]
        cmd: FactCmd,
    },
    /// Rows from `agent_pr`: pull requests a session touched.
    #[cfg(feature = "agent-read")]
    /// Folded (audit 2026-08-25): one-table dump; `boop db "SELECT * FROM agent_pr"` answers it.
    #[command(hide = true)]
    Pr {
        #[command(subcommand)]
        cmd: FactCmd,
    },
    /// Rows from `agent_span`: live time spans a session recorded.
    #[cfg(feature = "agent-read")]
    /// Folded (audit 2026-08-25): one-table dump; `boop db "SELECT * FROM agent_span"` answers it.
    #[command(hide = true)]
    Span {
        #[command(subcommand)]
        cmd: FactCmd,
    },
    /// Rows from `agent_edge`: parent/child spawn edges between sessions.
    /// Folded (audit 2026-08-25): one-table dump; `boop db "SELECT * FROM agent_edge"` answers it.
    #[command(hide = true)]
    Edge {
        #[command(subcommand)]
        cmd: EdgeCmd,
    },
    /// Tokens and cost. A totals report the passthrough powers, and a parent
    /// of the row computations blocks and burn-rate; clap needs both attributes
    /// to accept the two forms.
    #[cfg(feature = "agent-read")]
    #[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
    #[command(hide = true)]
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
    #[command(hide = true)]
    Price {
        #[command(subcommand)]
        cmd: PriceCmd,
    },
    /// User-pinned markdown: save a message you want to keep, read it back.
    #[cfg(feature = "agent-read")]
    #[command(hide = true)]
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
    #[command(hide = true)]
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
        /// Where the pass delivers the native-child completions it finds. A
        /// scratch mailbox here keeps an analysis pass off the live bus.
        #[arg(long)]
        mail_dir: Option<PathBuf>,
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
    /// One favorite by id, body included.
    Show {
        id: i64,
        #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
        format: QueryFormat,
    },
    /// Rewrite the note and/or source of one favorite; the body is immutable.
    Edit {
        id: i64,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        source: Option<String>,
    },
    /// Drop one favorite by id; its markdown body stays cached.
    Delete { id: i64 },
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

    /// Every shell command the help text prints, extracted from the two help
    /// constants themselves. A line counts as an example when it *starts* with
    /// `boop`; a prose sentence that merely mentions the binary does not.
    /// Continuations end in a backslash, and an unbalanced quote absorbs the
    /// lines that close it.
    fn help_examples(text: &str) -> Vec<String> {
        let mut examples = Vec::new();
        let mut pending: Option<String> = None;
        for raw in text.lines() {
            let line = raw.trim();
            match pending.take() {
                Some(mut open) => {
                    open.push(' ');
                    open.push_str(line.trim_end_matches('\\').trim());
                    pending = Some(open);
                }
                None => match example_start(line) {
                    Some(at) => pending = Some(line[at..].trim_end_matches('\\').trim().to_owned()),
                    None => continue,
                },
            }
            let Some(open) = pending.take() else { continue };
            let unbalanced = open.matches('"').count() % 2 == 1;
            if raw.trim_end().ends_with('\\') || unbalanced {
                pending = Some(open);
                continue;
            }
            examples.push(open);
        }
        examples.extend(pending);
        examples
    }

    /// Where a command example starts in one help line: at column zero, or
    /// after a label that ends in a colon. A backticked mention inside prose
    /// is not an example and names no column.
    fn example_start(line: &str) -> Option<usize> {
        let at = line.find("boop")?;
        if line[at..] != *"boop" && !line[at..].starts_with("boop ") {
            return None;
        }
        let before = &line[..at];
        match before.trim_end().is_empty() || before.trim_end().ends_with(':') {
            true => Some(at),
            false => None,
        }
    }

    /// The argv one help example stands for. Column-aligned prose after the
    /// command is cut, placeholders take a value every type accepts, an
    /// alternation takes its first spelling, and optional brackets are opened
    /// so the flags inside them are parsed rather than skipped.
    fn example_argv(example: &str) -> Vec<String> {
        let cut = example
            .find("   ")
            .or_else(|| example.find(" -- "))
            .map_or(example, |at| &example[..at]);
        let mut argv = Vec::new();
        let mut token = String::new();
        let mut quoted = false;
        for character in cut.chars() {
            match character {
                '"' => quoted = !quoted,
                '[' | ']' => {}
                character if character.is_whitespace() && !quoted => {
                    if !token.is_empty() {
                        argv.push(std::mem::take(&mut token));
                    }
                }
                character => token.push(character),
            }
        }
        if !token.is_empty() {
            argv.push(token);
        }
        argv.into_iter()
            .map(|word| match word.split_once('|') {
                Some((first, _)) => first.to_owned(),
                None => word,
            })
            .map(|word| match word.starts_with('<') && word.ends_with('>') {
                true => "1".to_owned(),
                false => word,
            })
            .collect()
    }

    // FAIL-PRE-FIX: the WAIT block printed `boop beep lane wait <lane>
    // --wait-timeout <s>`, a flag `beep lane wait` never had, and nothing
    // checked the help text against the parser that has to accept it.
    #[test]
    fn every_help_example_parses_through_clap() {
        let doctrine = doctrine();
        #[allow(unused_mut)]
        let mut examples = help_examples(&doctrine);
        #[cfg(feature = "dl6")]
        examples.extend(help_examples(CONCATMAP_EXAMPLES));
        // A regression in the extractor would pass this test by finding
        // nothing, so the count is asserted before the parses are. The floor
        // covers doctrine alone; `--features dl6` adds CONCATMAP_EXAMPLES on top.
        assert!(
            examples.len() >= 15,
            "the extractor found almost nothing: {examples:#?}"
        );
        let mut rejected = Vec::new();
        for example in &examples {
            let argv = example_argv(example);
            if let Err(error) = Cli::try_parse_from(&argv) {
                rejected.push(format!("{example}\n  argv: {argv:?}\n  {error}"));
            }
        }
        assert!(
            rejected.is_empty(),
            "help examples the installed parser rejects:\n{}",
            rejected.join("\n\n")
        );
    }

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

    /// RECEIPT (boop-db-convoy): failed pre-fix, no hatch existed and every
    /// read verb paid the cold sync.
    #[test]
    fn the_no_sync_hatch_clears_every_verb_that_would_sync() {
        let syncing = [
            vec!["boop", "db", "SELECT 1"],
            vec!["boop", "db", "turn", "list"],
            vec!["boop", "debug"],
            vec!["boop", "sessions"],
        ];
        for argv in syncing {
            let cli = Cli::try_parse_from(&argv).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
            assert!(
                startup_sync_wanted(cli.command.as_ref().unwrap(), false),
                "{argv:?} syncs by default"
            );
            assert!(
                !startup_sync_wanted(cli.command.as_ref().unwrap(), true),
                "{argv:?} must skip the sync under the hatch"
            );
        }
    }

    /// RECEIPT. `--bin` reaches both legs of the lane pair: `lane create`
    /// parses it into the spawn args, and `lane run` (the line the pane runs)
    /// parses it back out.
    #[test]
    fn lane_create_and_lane_run_both_take_a_bin_override() {
        let cli = Cli::try_parse_from([
            "boop",
            "beep",
            "lane",
            "create",
            "--lane",
            "bin-probe",
            "--preset",
            "zfable",
            "--bin",
            "ccz",
        ])
        .expect("parse lane create --bin");
        match cli.command {
            Some(SubCmd::Beep {
                cmd:
                    Some(BeepCmd::Lane {
                        cmd: LaneCmd::Create { bin, preset, .. },
                    }),
                ..
            }) => {
                assert_eq!(bin.as_deref(), Some("ccz"));
                assert_eq!(preset.as_deref(), Some("zfable"));
            }
            other => panic!("lane create parsed as {:?}", other.is_some()),
        }
        let cli = Cli::try_parse_from([
            "boop",
            "beep",
            "lane",
            "run",
            "--lane",
            "bin-probe",
            "--harness",
            "claude",
            "--brief",
            "/tmp/brief.md",
            "--bin",
            "ccz",
        ])
        .expect("parse lane run --bin");
        match cli.command {
            Some(SubCmd::Beep {
                cmd:
                    Some(BeepCmd::Lane {
                        cmd: LaneCmd::Run { bin, .. },
                    }),
                ..
            }) => assert_eq!(bin.as_deref(), Some("ccz")),
            other => panic!("lane run parsed as {:?}", other.is_some()),
        }
    }

    #[test]
    fn the_no_sync_hatch_never_makes_a_registry_verb_sync() {
        let cli = Cli::try_parse_from(["boop", "beep", "lane", "list"]).expect("parse");
        assert!(!startup_sync_wanted(cli.command.as_ref().unwrap(), false));
        assert!(!startup_sync_wanted(cli.command.as_ref().unwrap(), true));
    }

    #[test]
    fn help_text_names_the_no_sync_hatch() {
        let help = Cli::command().render_long_help().to_string();
        assert!(
            help.contains(NO_SYNC_ENV),
            "help text missing {NO_SYNC_ENV}:\n{help}"
        );
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

    /// RECEIPT (db-four-verbs): `usage`, `price`, `favorite`, `sync-cursor`
    /// hidden; `sql`, `chat`, `status`, `sync` are the four the reader sees.
    #[test]
    fn db_help_lists_exactly_chat_status_sync_besides_the_sql_passthrough() {
        let db = DbCmd::augment_subcommands(clap::Command::new("db"));
        let visible: std::collections::BTreeSet<String> = db
            .get_subcommands()
            .filter(|sub| !sub.is_hide_set())
            .map(|sub| sub.get_name().to_owned())
            .collect();
        assert_eq!(
            visible,
            ["chat", "status", "sync"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            "boop db --help subcommands: {visible:?}"
        );
    }

    /// RECEIPT (hide-lane-run): the supervisor's own entry point, spawned by
    /// `lane create`; no human types it.
    #[test]
    fn beep_lane_run_is_hidden_a_human_never_calls_it() {
        let lane = LaneCmd::augment_subcommands(clap::Command::new("lane"));
        let run = lane.find_subcommand("run").expect("run subcommand exists");
        assert!(run.is_hide_set(), "beep lane run must stay hidden");
    }

    /// RECEIPT (one-wait-verb): `boop wait <id>` dispatches to `run_lane_wait`
    /// only when `<id>` names a registered lane route, never a message id.
    #[test]
    fn wait_dispatches_to_lane_wait_only_for_a_registered_lane_route() {
        let dir = crate::cli::testkit::temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        crate::cli::write_route(&dir, "worker", crate::cli::testkit::route_with(None)).unwrap();
        assert!(wait_target_is_a_lane(Some(&dir), "worker"));
        assert!(!wait_target_is_a_lane(Some(&dir), "m-does-not-exist"));
        let _ = std::fs::remove_dir_all(&dir);
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
