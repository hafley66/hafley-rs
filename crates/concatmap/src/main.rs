//! The `concatmap` binary: one resident pipe per source session. Reads rules
//! from a toml file, reads visible turns from the boop store, drives a resident
//! opencode chat through tmux, folds replies into the agent's state file, and
//! commits each fold. v1 runs one pass per invocation (the resident loop shell
//! is the outer driver until the push-based watcher seam lands).

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};

use concatmap::cursor::Cursor;
use concatmap::host::{Dl6Host, Host};
use concatmap::interp::{Effects, MuxSink};
use concatmap::pipe;
use concatmap::rules;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "concatmap=info".into()),
        )
        .init();

    let args = Args::parse()?;

    let store = boop::open_default().context("open boop store")?;

    let state_dir = args.worktree.join("state");
    std::fs::create_dir_all(&state_dir)
        .with_context(|| format!("create state dir {}", state_dir.display()))?;
    let state_path = state_dir.join(format!("{}.dl6", args.agent));
    let cursor_path = state_dir.join("cursor");
    let offset_path = state_dir.join(format!("{}.offset", args.agent));

    let rules = rules::load_rules(&args.rules)?;
    let mut host = Host::new(&args.agent, rules, state_path.clone())?;
    let mut cursor = Cursor::load(&cursor_path)?;

    let mux = boop_mux::Tmux;
    let sink = MuxSink::new(&mux, args.socket.as_deref(), args.pane.clone());
    let effects = Effects {
        sink,
        worktree: args.worktree.clone(),
    };

    // One pass: read new turns since the cursor, bundle pairs, route each,
    // send, wait for the reply to settle, fold, commit, advance the cursor.
    let turns = pipe::read_new_turns(&store, &args.session, cursor.max_ts)?;
    let pairs = pipe::bundle_pairs(&turns);
    for pair in pairs {
        host.ingest_pair(pair.clone());
        let actions = host.evaluate()?;
        let sent = actions
            .iter()
            .any(|action| matches!(action, concatmap::Action::Send { .. }));
        for action in &actions {
            effects.apply(action)?;
        }
        if sent {
            let from = load_offset(&offset_path)?;
            match pipe::wait_reply(
                &args.transcript,
                from,
                Duration::from_secs(args.settle_secs),
                Duration::from_secs(args.reply_timeout),
            ) {
                Ok(Some((reply, next))) => {
                    save_offset(&offset_path, next)?;
                    pipe::fold_and_commit(&mut host, &effects, &reply, "fold")?;
                }
                _ => {
                    if let Some(cancelled) = host.cancel_in_flight() {
                        tracing::warn!(
                            session = cancelled.session,
                            turn = cancelled.turn,
                            "reply timed out; pair skipped"
                        );
                    }
                }
            }
        }
        cursor.observe(pair.turn);
    }
    cursor.save(&cursor_path)?;
    Ok(())
}

/// The transcript tail offset persists beside the state file; a missing file
/// starts at 0.
fn load_offset(path: &std::path::Path) -> Result<u64> {
    match std::fs::read_to_string(path) {
        Ok(text) => text.trim().parse().with_context(|| format!("parse {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error).with_context(|| format!("read {}", path.display())),
    }
}

fn save_offset(path: &std::path::Path, offset: u64) -> Result<()> {
    std::fs::write(path, offset.to_string())
        .with_context(|| format!("write {}", path.display()))
}

/// CLI args for one pass. v1 keeps these explicit; a future watcher-driven loop
/// seeds them from the spawn edge.
struct Args {
    agent: String,
    session: String,
    pane: String,
    socket: Option<String>,
    worktree: PathBuf,
    rules: PathBuf,
    transcript: PathBuf,
    settle_secs: u64,
    reply_timeout: u64,
}

impl Args {
    fn parse() -> Result<Args> {
        let mut args = std::env::args().skip(1);
        let mut agent = None;
        let mut session = None;
        let mut pane = None;
        let mut socket = None;
        let mut worktree = None;
        let mut rules = None;
        let mut transcript = None;
        let mut settle_secs = 15u64;
        let mut reply_timeout = 300u64;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--agent" => agent = Some(args.next().context("--agent needs a value")?),
                "--session" => session = Some(args.next().context("--session needs a value")?),
                "--pane" => pane = Some(args.next().context("--pane needs a value")?),
                "--socket" => socket = Some(args.next().context("--socket needs a value")?),
                "--worktree" => {
                    worktree = Some(args.next().context("--worktree needs a value")?)
                }
                "--rules" => rules = Some(args.next().context("--rules needs a value")?),
                "--transcript" => {
                    transcript = Some(args.next().context("--transcript needs a value")?)
                }
                "--settle-secs" => {
                    settle_secs = args
                        .next()
                        .context("--settle-secs needs a value")?
                        .parse()
                        .context("--settle-secs is not an integer")?
                }
                "--reply-timeout" => {
                    reply_timeout = args
                        .next()
                        .context("--reply-timeout needs a value")?
                        .parse()
                        .context("--reply-timeout is not an integer")?
                }
                other => anyhow::bail!("unknown argument {other}"),
            }
        }
        Ok(Args {
            agent: agent.context("missing --agent")?,
            session: session.context("missing --session")?,
            pane: pane.context("missing --pane")?,
            socket,
            worktree: worktree.context("missing --worktree").map(PathBuf::from)?,
            rules: rules.context("missing --rules").map(PathBuf::from)?,
            transcript: transcript
                .context("missing --transcript")
                .map(PathBuf::from)?,
            settle_secs,
            reply_timeout,
        })
    }
}
