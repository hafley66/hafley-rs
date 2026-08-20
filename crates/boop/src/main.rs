//! The `boop` CLI: two verb trees over five library crates. `beep` drives
//! agents, `db` reads what they did.
//!
//! No `match` on harness id and no direct `Command::new("tmux")` live here;
//! both belong to boop-harness and boop-mux.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use tracing_subscriber::EnvFilter;

use boop::registry::Registry;
use boop::supervise::ParentDeathPolicy;
use boop::{bus, config, identity, mailwait, proc};

mod cli;

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
    #[command(subcommand)]
    command: SubCmd,
}

#[derive(Subcommand)]
enum SubCmd {
    /// Drive agents: spawn lanes, hail them, register coordinators, measure them.
    ///
    /// Every verb that CHANGES something lives here. A coordinator calls
    /// `beep lane create` to spawn, `beep hail` to talk, `beep ps`/`beep pstree`
    /// to see what is alive. Reading what an agent DID is `boop db`.
    Beep {
        #[command(subcommand)]
        cmd: BeepCmd,
    },
    /// Read what agents did: raw SQL over ~/.agent/boop.db, or a typed row reader.
    ///
    /// Two forms. `boop db "<sql>"` runs one read-only statement; sqlite3
    /// dot-commands (.schema, .tables) are not accepted, plain SQL only.
    /// `boop db <sub>` is a named reader over the same tables.
    ///
    /// This store holds transcripts, facts and traces. It is NOT the mailbox:
    /// mail is NDJSON at ~/.agent/mail/bus.ndjson with routes in registry.json.
    #[command(
        args_conflicts_with_subcommands = true,
        subcommand_negates_reqs = true,
        after_help = "EXAMPLES:
  boop db \"SELECT name FROM sqlite_master WHERE type='table'\"
  boop db turn list --limit 5 --format text
  boop db status --window 10"
    )]
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
    /// What just went wrong: WARN/ERROR from every lane trail and the store.
    ///
    /// Reads the tail of every ~/.agent/lanes/<lane>/supervise.log plus the
    /// store's kind=error trace rows, grouped by lane, oldest first inside a
    /// lane. `boop --help` prints a one-line banner when this window is
    /// non-empty and nothing when it is clean.
    #[command(after_help = "EXAMPLES:
  boop debug --since 10m
  boop debug --lane fix-help-sweep --json")]
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
    /// Sync transcripts, then print the CASS-compatible agent/runtime summary.
    ///
    /// A read verb for a dashboard or another tool, not for driving agents.
    /// The spawn/register verbs are `boop beep agent`.
    #[command(after_help = "EXAMPLES:
  boop agent summary --format text
  boop agent sessions --cwd ~/projects/hafley-rs")]
    #[cfg(feature = "agent-read")]
    Agent {
        #[command(subcommand)]
        cmd: AgentSummaryCmd,
    },
    /// Refinement loop: map each new (assistant, user) pair through a model pass.
    ///
    /// Runs in the foreground and writes one rewrite per turn. For a resident
    /// DL6 coroutine instead, use `boop host chat`. This page's TEMPLATE,
    /// STATE and RULES blocks define the three file inputs.
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
    /// Typed stdin/stdout boundary a compiled DL6 program calls a model through.
    ///
    /// The program writes one JSON request on stdin and reads one JSON
    /// response on stdout. Nothing else reads or writes the pipe.
    Host {
        #[command(subcommand)]
        cmd: HostCmd,
    },
    /// Mail your own parent; the ladder names both ends, so neither is typed.
    ///
    /// Called BY a lane or a subagent, never at one. The identity ladder names
    /// the sender and the route's `parent` edge names the recipient. The row is
    /// appended and then handed to the same delivery ladder `beep hail` uses.
    #[command(after_help = "EXAMPLES:
  boop tell-parent --kind completion --body \"PR #44 posted, gate green\"
  boop tell-parent --kind yield")]
    TellParent {
        /// What the row says it is. `yield` carries a default body.
        #[arg(long, default_value = "note", value_parser = ["completion", "yield", "note"])]
        kind: String,
        /// The message. Required for every kind but `yield`.
        #[arg(long)]
        body: Option<String>,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Mail one body to every live child of the caller, one line per target.
    ///
    /// Children come from the registry's `parent` edges plus the store's
    /// `spawned` edges for the caller's session. Each target reports landed or
    /// dead and the run ends in a tally, so a run that reached nobody cannot
    /// read as success.
    #[command(after_help = "EXAMPLES:
  boop tell-children --body \"stop and report where you are\"")]
    TellChildren {
        /// The message every child gets.
        #[arg(long)]
        body: String,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Print the caller's own name and which rung of the ladder resolved it.
    ///
    /// The ladder, in order: $BOOP_SESSION, the tmux pane the caller sits in,
    /// then the process tree. The name it prints is what `--as` and `--from`
    /// take everywhere else. An empty answer means nothing can address you.
    Whoami {
        /// One JSON object instead of the text lines.
        #[arg(long)]
        json: bool,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Block until mail lands: the reply to <id>, or your next unread row.
    ///
    /// The universal pull. Every agent can background a shell, so a block is
    /// the one transport that works from any harness with no route, no pane
    /// and no hook. A reply is a row naming your id in `reply_to`, or the
    /// recipient's next mail back to you. Every arrival is printed and stamped
    /// delivered, so a second wait on the same id blocks instead of replaying.
    /// A timeout exits 124. The LAST line of every exit is the next command.
    #[command(after_help = "EXAMPLES:
  boop wait <message-id>
  boop wait --me --wait-timeout 120")]
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
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// The pull side of mail: what a claude coordinator drains at a turn edge.
    ///
    /// With the two hooks installed, `deliver_hail` pushes NOTHING at that
    /// name: the receiver's own Stop and UserPromptSubmit hooks run
    /// `boop inbox drain` and the mail arrives as part of a turn. Removing the
    /// hooks restores pane injection for that name.
    #[command(after_help = "EXAMPLES:
  boop inbox hooks --name sprefa-coordinator
  boop inbox drain --as sprefa-coordinator")]
    Inbox {
        #[command(subcommand)]
        cmd: InboxCmd,
    },
    /// Register the Codex pane you are sitting in, or act on your conversation.
    ///
    /// Bare `boop me` writes a kind=coordinator route for the current tmux
    /// pane so mail can reach you. It is CODEX ONLY: it fails unless a root
    /// Codex transcript records this directory, and it stamps harness=codex.
    /// Any other harness registers through `boop adopt --name <n> --tmux <t>`.
    /// The subcommands act on whatever conversation the whoami ladder resolves.
    #[command(
        args_conflicts_with_subcommands = true,
        subcommand_negates_reqs = true,
        after_help = "EXAMPLES:
  boop me --name codex-main
  boop me mood unga
  boop me favorite -1 --note \"the delivery table\""
    )]
    Me {
        /// Registry name; defaults to `codex-<pane id>` with the leading % cut.
        #[arg(long)]
        name: Option<String>,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
        #[command(subcommand)]
        cmd: Option<MeCmd>,
    },
    /// Print the boop config the CLI reads: its path, its JSON, its presets.
    ///
    /// Read-only. Editing is done in the file `boop config path` prints.
    #[command(after_help = "EXAMPLES:
  boop config path
  boop config presets")]
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
        /// How each event row is rendered.
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Stream turns across sessions, filtered from the db. (pass 4)
    #[command(hide = true)]
    Events {
        #[command(flatten)]
        query: QueryArgs,
    },
    /// Every route, then every open message. Superseded by `beep lane list`.
    #[command(hide = true)]
    List {
        /// Only this lane's rows.
        #[arg(long)]
        agent: Option<String>,
        /// Every message ever, acked included; default is open rows only.
        #[arg(long)]
        all: bool,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Measure per-lane pid, rss, cpu, uptime, child count.
    #[command(hide = true)]
    Measure {
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Spawn a lane: tmux new-session + mailbox + registry route.
    #[command(hide = true)]
    Dispatch {
        /// Lane id to spawn and register.
        #[arg(long)]
        to: String,
        /// Directory the spawned pane starts in.
        #[arg(long)]
        cwd: String,
        /// The shell command the pane runs.
        #[arg(long)]
        cmd: String,
        /// Sender recorded on the opening message.
        #[arg(long)]
        from: Option<String>,
        /// Harness id recorded on the route.
        #[arg(long)]
        harness: Option<String>,
        /// Harness conversation id recorded on the route.
        #[arg(long)]
        session_id: Option<String>,
        /// Model spelling recorded on the route.
        #[arg(long)]
        model: Option<String>,
        /// Mail-rendering mood recorded on the route.
        #[arg(long)]
        mode: Option<String>,
        /// tmux session name; defaults to the lane id.
        #[arg(long)]
        tmux: Option<String>,
        /// tmux socket to spawn on; the default server when absent.
        #[arg(long)]
        socket: Option<String>,
        /// First message queued for the new lane.
        #[arg(long)]
        body: Option<String>,
        /// Correlation id copied onto the queued message.
        #[arg(long)]
        r#ref: Option<String>,
        /// What the lane is running toward.
        #[arg(long)]
        goal: Option<String>,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
        /// Seconds to sleep after the spawn, before returning.
        #[arg(long, default_value_t = 3)]
        resolve_wait: u64,
        /// Spawn in the main tree instead of creating a worktree.
        #[arg(long)]
        main_tree: bool,
        /// Commit the worktree is cut at.
        #[arg(long)]
        base_sha: Option<String>,
    },
    /// Resolve a lane's harness session id into its registry route.
    #[command(hide = true)]
    Resolve {
        /// Lane id whose route is resolved.
        #[arg(long)]
        to: String,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Queue a message and inject it into a live pane.
    #[command(hide = true)]
    Hail {
        /// Recipient route name.
        #[arg(long)]
        to: String,
        /// The message text.
        #[arg(long)]
        body: String,
        /// Sender name; defaults to `coordinator`.
        #[arg(long)]
        from: Option<String>,
        /// Row kind; defaults to `request`. Only request/hail/note/retry/resume reach a lane.
        #[arg(long)]
        kind: Option<String>,
        /// NDJSON file inside the mail dir; defaults to bus.ndjson.
        #[arg(long)]
        box_: Option<String>,
        /// tmux socket the recipient's pane lives on.
        #[arg(long)]
        socket: Option<String>,
        /// Send, then block for the reply exactly as `boop wait <id>` does.
        #[arg(long, value_name = "SECS")]
        wait_timeout: Option<u64>,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Acknowledge unacked lanes via cass and stamp token usage.
    #[command(hide = true)]
    Sweep {
        /// Only this lane's mail.
        #[arg(long)]
        agent: Option<String>,
        /// NDJSON file inside the mail dir; defaults to bus.ndjson.
        #[arg(long)]
        box_: Option<String>,
        /// Also ack mail addressed to names with no registry route.
        #[arg(long)]
        close_routeless: bool,
        /// Rows older than this are acked outright, as expired.
        #[arg(long, default_value_t = 7)]
        max_age_days: u64,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Register and spawn a lane (the first-contact verb).
    #[command(hide = true)]
    Lane {
        /// Lane id and registry key.
        #[arg(long)]
        name: String,
        /// Repo the lane works in.
        #[arg(long)]
        cwd: String,
        /// Harness id; defaults to the one the model spelling names.
        #[arg(long)]
        harness: Option<String>,
        /// Absolute path to the brief the lane reads and executes.
        #[arg(long)]
        brief: Option<PathBuf>,
        /// Model spelling, in the harness's own flag form.
        #[arg(long)]
        model: Option<String>,
        /// Named model preset from boop/config.json; `boop config presets` lists them.
        #[arg(long, conflicts_with = "model")]
        preset: Option<String>,
        /// tmux session name; defaults to the lane id.
        #[arg(long)]
        tmux: Option<String>,
        /// Route that receives this lane's result row.
        #[arg(long)]
        parent: Option<String>,
        /// New branch name; with `--base-sha`, spawns in a worktree instead
        /// of `--cwd` directly.
        #[arg(long)]
        branch: Option<String>,
        /// Commit the worktree is cut at.
        #[arg(long)]
        base_sha: Option<String>,
        /// tmux socket to spawn on; a throwaway socket for tests, `None` for
        /// the default server.
        #[arg(long)]
        socket: Option<String>,
        /// What the lane is running toward.
        #[arg(long)]
        goal: Option<String>,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
        /// Print the spawn line and register nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Register an interactive pane as a coordinator route. Never spawns.
    ///
    /// `--harness claude` also installs the two drain hooks in the pane's
    /// project settings, which FLIPS that name from push to pull: hails to it
    /// stop being typed at the pane and wait for its own `boop inbox drain`
    /// at a turn boundary. Every other harness keeps pane injection.
    /// `--no-hooks` keeps injection for a claude pane too.
    #[command(hide = true)]
    Adopt {
        /// Registry name for the adopted pane.
        #[arg(long)]
        name: String,
        /// tmux session or pane the route points at.
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
        /// Harness running in the pane; `claude` also gets the hook inbox.
        #[arg(long)]
        harness: Option<String>,
        /// Harness conversation id; discovered from the pane when absent.
        #[arg(long)]
        session_id: Option<String>,
        /// Project whose .claude/settings.json carries the drain hooks.
        #[arg(long)]
        cwd: Option<String>,
        /// Model spelling recorded on the route.
        #[arg(long)]
        model: Option<String>,
        /// Mail-rendering mood recorded on the route.
        #[arg(long)]
        mode: Option<String>,
        /// The lane that summoned this one.
        #[arg(long)]
        parent: Option<String>,
        /// What this coordinator is working toward.
        #[arg(long)]
        goal: Option<String>,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Drop lanes whose tmux sessions are gone. Refuses when tmux is
    /// unreachable because it cannot tell live from dead.
    #[command(hide = true)]
    Prune {
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
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
    /// Only rows from this harness id.
    #[arg(long)]
    harness: Option<String>,
    /// Only rows from this harness session id.
    #[arg(long)]
    session: Option<String>,
    /// Only rows with this role: user or assistant.
    #[arg(long)]
    role: Option<String>,
    /// Lower bound on row ts, Unix ms.
    #[arg(long)]
    since: Option<u64>,
    /// Upper bound on row ts, Unix ms.
    #[arg(long)]
    until: Option<u64>,
    /// Lower bound on the turn ordinal.
    #[arg(long)]
    turn_from: Option<u64>,
    /// Upper bound on the turn ordinal.
    #[arg(long)]
    turn_to: Option<u64>,
    /// Only sessions that touched this path.
    #[arg(long)]
    path: Option<String>,
    /// Cap the row count.
    #[arg(long)]
    limit: Option<u64>,
    /// How each row is rendered.
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
    /// Print the config file path this machine resolves to.
    Path,
    /// Print the loaded config as pretty JSON, defaults included.
    Show,
    /// One row per `--preset` name: model, variant, harness, and the default.
    Presets,
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
                // No lane supervisor polls an adopted pane's mailbox, so the
                // route kind must be one `deliver_hail` pushes to itself.
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
    /// The harness adapters: which ids `--harness` accepts, and what each drives.
    Harness {
        #[command(subcommand)]
        cmd: HarnessCmd,
    },
    /// Lanes: spawn one, list them, hail them, wait on one, tear one down.
    ///
    /// A lane is a tmux pane running `beep lane run`, which owns the harness
    /// conversation and reads the lane's mailbox itself. `lane create` is the
    /// only spawn door; a bare tmux spawn leaves no route and no parent edge.
    Lane {
        #[command(subcommand)]
        cmd: LaneCmd,
    },
    /// Register pane-less names: a coordinator, or an Agent-tool subagent.
    ///
    /// A `native` row makes the name ADDRESSABLE and mailable-in-principle,
    /// with no transport behind it: an Agent-tool child has no route of its
    /// own, no pane, no stdin and no ACP session, so every hail to it takes
    /// the "queued (no pane)" arm and is read only by `boop wait`.
    Agent {
        #[command(subcommand)]
        cmd: AgentCmd,
    },
    /// Send one message to a running agent, and say how it was delivered.
    ///
    /// Appends the row, then picks a transport from the recipient's route:
    /// a lane is left for its own supervisor, a hook-installed name is left
    /// for its `boop inbox drain`, an otherwise-live pane is typed into, and
    /// anything else stays queued for `boop wait`. The printed line names
    /// which arm ran. `boop --help` carries the whole table.
    #[command(after_help = "EXAMPLES:
  boop beep hail fix-parser --body \"rebase onto origin/main first\"
  boop beep hail fix-parser --body \"status?\" --wait-timeout 120")]
    Hail {
        /// The lane to hail.
        lane: String,
        /// The message text.
        #[arg(long)]
        body: String,
        /// Sender name; defaults to `coordinator`.
        #[arg(long)]
        from: Option<String>,
        /// Row kind; defaults to `request`. Only request/hail/note/retry/resume reach a lane.
        #[arg(long)]
        kind: Option<String>,
        /// tmux socket the recipient's pane lives on.
        #[arg(long)]
        socket: Option<String>,
        /// Send, then block for the reply exactly as `boop wait <id>` does.
        #[arg(long, value_name = "SECS")]
        wait_timeout: Option<u64>,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Bulk mail housekeeping. Reading one lane's mail is `beep lane message`.
    Message {
        #[command(subcommand)]
        cmd: MessageCmd,
    },
    /// pid, rss, cpu, uptime and child count per live lane.
    ///
    /// Half of the liveness test. The other half is `git -C <worktree> status
    /// --short`: a lane can hold a live process and change nothing.
    Ps {
        /// One lane only; every live lane when absent.
        lane: Option<String>,
        /// Include dead routes (no live process behind the pane).
        #[arg(long)]
        all: bool,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Tree of lanes by parent edge, filesystem style.
    Pstree {
        /// Include dead lanes; default is live-only.
        #[arg(long)]
        all: bool,
        /// How the tree is rendered.
        #[arg(long, value_enum, default_value_t = PstreeFormat::Text)]
        format: PstreeFormat,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum HarnessCmd {
    /// One line per registered adapter: the ids `--harness` accepts.
    List,
    /// One adapter's id, transcript root, and what it can be driven with.
    Get {
        /// Harness id, as `boop beep harness list` spells it.
        harness: String,
    },
}

#[derive(Subcommand)]
enum HostCmd {
    /// Read one JSON request from stdin, emit one JSON response on stdout.
    Chat,
}

#[derive(Subcommand)]
enum LaneCmd {
    /// One row per registered lane, with live or dead.
    List {
        /// Only lanes in this state: `live`, `dead`, or `?` when tmux is unreachable.
        #[arg(long)]
        state: Option<String>,
        /// Only lanes on this harness id.
        #[arg(long)]
        harness: Option<String>,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// The spawn door: cut a worktree, start the agent, register the route.
    ///
    /// ONE derivation from the whole branch name. `feature/schema-emit` gives
    /// lane id and tmux session `feature-schema-emit` (`/` spelled `-`, the one
    /// character tmux cannot hold) and worktree
    /// `.boop-worktrees/feature/schema-emit`. No prefix is dropped, no `lane/`
    /// prefix is added. Run `--dry-run` first for any shape you have not used;
    /// the printed `cmd:` line is the literal spawn.
    #[command(after_help = "EXAMPLES:
  boop beep lane create --branch fix/parser --brief /abs/BRIEF.md --dry-run
  boop beep lane create --branch fix/parser --brief /abs/BRIEF.md --preset flash4 --wait")]
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
        /// Model spelling, in the harness's own flag form.
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
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
        /// Print the spawn line and register nothing.
        #[arg(long)]
        dry_run: bool,
        /// Remove a dead lane's worktree and branch before spawning. A live
        /// route or a live pane on the name refuses.
        #[arg(long)]
        reclaim: bool,
    },
    /// The supervisor a lane pane runs. Spawn with `lane create`, never this.
    ///
    /// Owns the harness conversation over ACP and reads the lane's mailbox on
    /// a 700 ms poll. It opens the conversation with the brief and holds every
    /// hail for a resume turn, because ACP has no mid-turn prompt. Nothing is
    /// dropped and no hail needs a human re-dispatch.
    Run {
        /// Lane id this supervisor owns.
        #[arg(long)]
        lane: String,
        /// Harness id to open the conversation on.
        #[arg(long)]
        harness: String,
        /// Absolute path to the brief that opens the conversation.
        #[arg(long)]
        brief: PathBuf,
        /// Model spelling, in the harness's own flag form.
        #[arg(long)]
        model: Option<String>,
        /// Continue an existing harness conversation instead of opening one.
        #[arg(long)]
        resume: Option<String>,
        /// opencode reasoning-effort variant, threaded from `lane create`.
        #[arg(long)]
        variant: Option<String>,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// One lane's route row and whether it is live.
    Get {
        /// The lane to report on.
        lane: String,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Repoint an existing lane's route at a pane that already exists.
    Patch {
        /// The lane whose route is repointed.
        lane: String,
        /// tmux session or pane the route points at.
        #[arg(long)]
        tmux: String,
        /// Harness running in the pane.
        #[arg(long)]
        harness: Option<String>,
        /// Harness conversation id recorded on the route.
        #[arg(long)]
        session_id: Option<String>,
        /// Directory recorded on the route; also where a drain hook is looked for.
        #[arg(long)]
        cwd: Option<String>,
        /// Model spelling recorded on the route.
        #[arg(long)]
        model: Option<String>,
        /// Mail-rendering mood recorded on the route.
        #[arg(long)]
        mode: Option<String>,
        /// The lane that summoned this one.
        #[arg(long)]
        parent: Option<String>,
        /// What the lane is running toward.
        #[arg(long)]
        goal: Option<String>,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Stop one lane and drop its route, or bulk-delete by state.
    Delete {
        /// The lane to stop and forget; use --state for a bulk delete.
        lane: Option<String>,
        /// Drop only the registry route; never kill the pane. The `--parent`
        /// on-exit epilogue uses this to clean up while still running inside it.
        #[arg(long)]
        route_only: bool,
        /// Delete every lane in this state instead of one by name.
        #[arg(long)]
        state: Option<String>,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Drop routes whose pane is gone and whose recorded pid is not alive.
    ///
    /// Refuses outright when tmux is unreachable, because it then cannot tell
    /// live from dead.
    Prune {
        /// Print what would be pruned; remove nothing.
        #[arg(long)]
        dry_run: bool,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// One lane's tmux target, harness session id, cwd and parent.
    Route {
        /// The lane whose route is printed.
        lane: String,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Capture the lane's screen. Proof of nothing on its own; see `beep ps`.
    Pane {
        /// The lane whose pane is captured.
        lane: String,
        /// How many scrollback lines to capture.
        #[arg(long)]
        lines: Option<u32>,
        /// tmux socket the pane lives on.
        #[arg(long)]
        socket: Option<String>,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// One lane's mailbox rows, sent and received.
    Message {
        #[command(subcommand)]
        cmd: LaneMessageCmd,
    },
    /// Block on a lane's result row, then exit with the rc that row names.
    ///
    /// A result row is APPEND-ONLY: nothing pushes it anywhere, so a caller
    /// that wants the rc polls for it here (or, for a claude coordinator with
    /// the drain hook, reads it at its next turn boundary). `--timeout`
    /// seconds exits 124; a route that dies with no row exits 3.
    Wait {
        /// The lane to wait on.
        lane: String,
        /// Seconds to wait before exiting 124; 0 waits until the lane reports
        /// or its route dies.
        #[arg(long, default_value_t = 0)]
        timeout: u64,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum AgentCmd {
    /// File a pane-less registry row so the name can be addressed.
    #[command(after_help = "EXAMPLE:
  boop beep agent register audit-1 --kind native --parent sprefa-coordinator")]
    Register {
        /// Registry name the new row is filed under; also the mail address.
        name: String,
        /// coordinator or native; anything else is refused.
        #[arg(long, default_value = "native")]
        kind: String,
        /// Route this agent's result row is addressed to.
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
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Append the agent's result row and drop its registry row.
    #[command(after_help = "EXAMPLE:
  boop beep agent done audit-1 --rc 0")]
    Done {
        /// Registry name of the row to close.
        name: String,
        /// Exit code the result row reports.
        #[arg(long, default_value_t = 0)]
        rc: i32,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
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
        /// How the summary is rendered.
        #[arg(long, value_enum, default_value_t = AgentSummaryFormat::Json)]
        format: AgentSummaryFormat,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
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
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
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
    /// Every mailbox row naming this lane, sent or received.
    List {
        /// The lane whose mailbox is listed.
        lane: String,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
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
        /// Which Claude Code hook is reading; it shapes the stdout payload.
        #[arg(long, value_enum, default_value_t = HookArg::Plain)]
        hook: HookArg,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Install (or remove) the two drain hooks in <cwd>/.claude/settings.json.
    /// `boop adopt --harness claude` does this for you.
    Hooks {
        /// Registry name the installed drain command passes to `--as`.
        #[arg(long)]
        name: String,
        /// The project whose settings carry the hooks; defaults to this dir.
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Remove the two hooks instead of adding them.
        #[arg(long)]
        uninstall: bool,
    },
}

#[derive(Subcommand)]
enum MessageCmd {
    /// Age-based bulk mark-as-handled. NOT proof of read, never of compliance.
    Ack {
        /// Only this lane's mail.
        #[arg(long)]
        lane: Option<String>,
        /// NDJSON file inside the mail dir; defaults to bus.ndjson.
        #[arg(long)]
        box_: Option<String>,
        /// Also ack mail addressed to names with no registry route.
        #[arg(long)]
        close_routeless: bool,
        /// Rows older than this are acked outright, as expired.
        #[arg(long, default_value_t = 7)]
        max_age_days: u64,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
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
        /// How the summary is rendered.
        #[arg(long, value_enum, default_value_t = AgentSummaryFormat::Json)]
        format: AgentSummaryFormat,
        /// Mailbox directory; defaults to ~/.agent/mail (bus.ndjson + registry.json).
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
    /// Tokens and dollars: totals bare, or a windowed report by subcommand.
    ///
    /// `--show-sql` prints the SQL a form runs and exits, so the numbers can be
    /// checked or the query reused through `boop db "<sql>"`.
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
    /// The USD-per-million-token rate rows every cost number is computed from.
    #[cfg(feature = "agent-read")]
    Price {
        #[command(subcommand)]
        cmd: PriceCmd,
    },
    /// Pinned markdown: save a message worth keeping, read it back later.
    #[cfg(feature = "agent-read")]
    Favorite {
        #[command(subcommand)]
        cmd: FavoriteCmd,
    },
    /// Ingest new transcript bytes. Every read verb already syncs first.
    Sync {
        #[command(subcommand)]
        cmd: SyncCmd,
    },
    /// The byte offset ingest has reached in each transcript file.
    #[cfg(feature = "agent-read")]
    SyncCursor {
        #[command(subcommand)]
        cmd: CursorCmd,
    },
    /// One row per session: alive or not, last movement, tokens and cost.
    #[cfg(feature = "agent-read")]
    Status {
        /// Window in minutes.
        #[arg(long, default_value_t = 10)]
        window: u64,
        /// How each row is rendered.
        #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
        format: QueryFormat,
    },
}

#[cfg(feature = "agent-read")]
#[derive(clap::Args, Clone, Default)]
struct UsageArgs {
    /// How each row is rendered.
    #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
    format: QueryFormat,
}

#[cfg(feature = "agent-read")]
#[derive(clap::Args, Clone, Default)]
struct FactArgs {
    /// Only rows from this harness session id.
    #[arg(long)]
    session: Option<String>,
    /// Lower bound on row ts, Unix ms.
    #[arg(long)]
    since: Option<u64>,
    /// Upper bound on row ts, Unix ms.
    #[arg(long)]
    until: Option<u64>,
    /// Prefix match on the row's leading dictionary column.
    #[arg(long)]
    like: Option<String>,
    /// Cap the row count.
    #[arg(long)]
    limit: Option<u64>,
    /// How each row is rendered.
    #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
    format: QueryFormat,
}

#[cfg(feature = "agent-read")]
#[derive(Subcommand)]
enum UsageCmd {
    /// Gap-aware billing windows.
    Blocks {
        /// Width of one billing window, in hours.
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
        /// Trailing window, in minutes.
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
        /// Model id the rate row is keyed on.
        model: String,
        /// USD per million input tokens.
        #[arg(long)]
        input_per_mtok: f64,
        /// USD per million output tokens.
        #[arg(long)]
        output_per_mtok: f64,
        /// USD per million tokens written to the 5-minute cache.
        #[arg(long)]
        cache_write_5m_per_mtok: f64,
        /// USD per million tokens written to the 1-hour cache.
        #[arg(long)]
        cache_write_1h_per_mtok: f64,
        /// USD per million tokens read from cache.
        #[arg(long)]
        cache_read_per_mtok: f64,
        /// What wrote this rate row.
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
        /// Cap the row count.
        #[arg(long)]
        limit: Option<u64>,
        /// How each row is rendered.
        #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
        format: QueryFormat,
    },
    /// The one `agent_session` row matching this session id.
    Get {
        /// The harness session id to fetch.
        session: String,
        /// How the row is rendered.
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
        /// The harness session id the turn belongs to.
        session: String,
        /// Turn ordinal inside that session.
        turn: u64,
        /// How the row is rendered.
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
        /// Project every session the registry knows, not just the filtered set.
        #[arg(long)]
        all: bool,
        /// Keep tailing, one NDJSON line per new turn.
        #[arg(long)]
        follow: bool,
    },
}

#[derive(Subcommand)]
enum EdgeCmd {
    /// Every `agent_edge` row, filtered to one session's edges when given.
    List {
        /// Only edges touching this session id.
        #[arg(long)]
        session: Option<String>,
        /// Cap the row count.
        #[arg(long)]
        limit: Option<u64>,
    },
}

#[derive(Subcommand)]
enum SyncCmd {
    /// Ingest new transcript bytes into the store's `agent_turn` and fact tables.
    Create {
        /// Drop every stored row and re-project every transcript from byte 0.
        #[arg(long)]
        rebuild: bool,
        /// Keep syncing on a poll instead of returning.
        #[arg(long)]
        forever: bool,
    },
}

#[derive(Subcommand)]
enum MeCmd {
    /// Read or set the format agents mail this session in.
    ///
    /// With no name it prints the effective mood and the session that set it.
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
    /// Pin one assistant turn of the caller's own conversation into the store.
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
        /// Cap the row count.
        #[arg(long)]
        limit: Option<u64>,
        /// How each row is rendered.
        #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
        format: QueryFormat,
    },
}

#[cfg(feature = "agent-read")]
#[derive(Subcommand)]
enum CursorCmd {
    /// Every `sync_cursor` row: how far ingest has read each transcript.
    List {
        /// Cap the row count.
        #[arg(long)]
        limit: Option<u64>,
        /// How each row is rendered.
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
            SubCmd::Agent {
                cmd: AgentSummaryCmd::Summary { .. }
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

    /// Every `boop ...` line the help prints, as one argv apiece. A page that
    /// shows a command an agent cannot run is worse than a page with none.
    const HELP_EXAMPLES: [&str; 26] = [
        "boop db \"SELECT name FROM sqlite_master WHERE type='table'\"",
        "boop db turn list --limit 5 --format text",
        "boop db status --window 10",
        "boop debug --since 10m",
        "boop debug --lane fix-help-sweep --json",
        "boop agent summary --format text",
        "boop agent sessions --cwd ~/projects/hafley-rs",
        "boop tell-parent --kind completion --body \"PR #44 posted, gate green\"",
        "boop tell-parent --kind yield",
        "boop tell-children --body \"stop and report where you are\"",
        "boop wait <message-id>",
        "boop wait --me --wait-timeout 120",
        "boop inbox hooks --name sprefa-coordinator",
        "boop inbox drain --as sprefa-coordinator",
        "boop me --name codex-main",
        "boop me mood unga",
        "boop me favorite -1 --note \"the delivery table\"",
        "boop config path",
        "boop config presets",
        "boop beep hail fix-parser --body \"rebase onto origin/main first\"",
        "boop beep lane create --branch fix/parser --brief /abs/BRIEF.md --dry-run",
        "boop beep agent done audit-1 --rc 0",
        "boop concatmap --session ses_abc123 --mode tighten --template tighten.md \
         --state ~/.agent/concatmap/tighten/state",
        "boop concatmap --me --mode tighten --template tighten.md --state s",
        "boop concatmap --me --rules rules.json --mode tighten --template tighten.md --state s",
        "boop concatmap --me --rules rules.json --state s",
    ];

    /// Split on whitespace, keeping a double-quoted run whole.
    fn argv(line: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut word = String::new();
        let mut quoted = false;
        for character in line.chars() {
            match character {
                '"' => quoted = !quoted,
                c if c.is_whitespace() && !quoted => {
                    if !word.is_empty() {
                        out.push(std::mem::take(&mut word));
                    }
                }
                c => word.push(c),
            }
        }
        if !word.is_empty() {
            out.push(word);
        }
        out
    }

    /// RECEIPT (boop-help-sweep): failed pre-fix on `boop wait 01J8XYZ...`,
    /// which the parser rejects as an unknown positional shape.
    #[test]
    fn every_help_example_parses() {
        for line in HELP_EXAMPLES {
            let words = argv(line);
            Cli::try_parse_from(&words).unwrap_or_else(|error| panic!("{line}: {error}"));
        }
    }

    /// The list above cannot drift from the pages: every entry must be printed
    /// somewhere in the tree's long help.
    #[test]
    fn every_help_example_is_printed_somewhere() {
        fn pages(cmd: &clap::Command, into: &mut String) {
            into.push_str(&cmd.clone().render_long_help().to_string());
            for sub in cmd.get_subcommands() {
                pages(sub, into);
            }
        }
        let mut text = String::new();
        pages(&Cli::command(), &mut text);
        let flat: String = text
            .split_whitespace()
            .filter(|word| *word != "\\")
            .collect::<Vec<_>>()
            .join(" ");
        for line in HELP_EXAMPLES {
            let needle = line.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(flat.contains(&needle), "help prints no example {line:?}");
        }
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
