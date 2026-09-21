//! `ryi cleave <SRC>#<ITEM> <DEST>`: one item leaves SRC and lands in DEST,
//! carrying the specifiers it needs, dropping the ones nothing left in SRC
//! references, and respelling every importer. TypeScript only; the corpus read,
//! the soopy stages and the verify rollback are `move`'s, reused as they are.
//! @comment-ok: module header, the seam list every bin arm opens with

use std::path::{Path, PathBuf};

use clap::Parser;
use sprefa_extract::move_stage::state_root;
use sprefa_extract::{normalize, MoveCx};

/// The out-of-scope list the help text states, so a caller reads it before the
/// run rather than after.
const SCOPE: &str = "Out of scope, each its own issue: Rust cleave (the `mod` relocation and \
                     visibility widening), cross-language cleave, moving a type together with \
                     its `impl` blocks, and an item whose free names carry a `-` grade (the \
                     names and the `ryi graph --uses` command that answers them print, exit 0).";

#[derive(Parser)]
#[command(
    name = "ryi cleave",
    about = "move one item out of a file into another, with the imports it needs",
    after_help = SCOPE
)]
pub struct CleaveCli {
    /// `<SRC>#<ITEM>`: the file the item is declared in and its name.
    target: String,
    /// The file it lands in. Created when it does not exist.
    dest: PathBuf,
    /// Corpus root. Defaults to the git root holding SRC.
    #[arg(long)]
    root: Option<PathBuf>,
    /// Soopy state root. Must sit outside the corpus root.
    #[arg(long)]
    state: Option<PathBuf>,
    /// Also pull the same-file private helpers the item references, to a
    /// fixpoint whose pass count is the plan's `drag_iterations`.
    #[arg(long)]
    drag: bool,
    /// Apply the plan to the real tree instead of dry running it.
    #[arg(long)]
    commit: bool,
    /// Run this shell command in the root after `--commit`; a non-zero or
    /// timed-out run rolls every touched file back to its pre-run state.
    #[arg(long = "verify")]
    verify: Option<String>,
    /// Report the SRC spellings this cleave leaves behind in plain text.
    #[arg(long = "text-refs")]
    text_refs: bool,
    /// Close the output with one JSON line carrying the whole plan.
    #[arg(long)]
    json: bool,
}

pub fn run<I>(args: I) -> Result<(), String>
where
    I: IntoIterator,
    I::Item: Into<std::ffi::OsString> + Clone,
{
    let cli = CleaveCli::try_parse_from(args).map_err(|error| error.to_string())?;
    if cli.verify.is_some() && !cli.commit {
        return Err(
            "--verify runs the command only after --commit; a dry run never runs it".to_string(),
        );
    }
    let (src, item) = split_target(&cli.target)?;
    let root = plan_root(cli.root.as_ref(), &src)?;
    let cx = MoveCx::open(&root)?;
    let src = within_root(&root, &anchor_file(&src)?)?;
    let dest = within_root(&root, &canonical_unborn(&absolute(&cli.dest)?))?;
    if !cx.contains(&src) {
        return Err(format!("cleave source is outside the corpus: {src}"));
    }
    if src == dest {
        return Err(format!("{src} is both the source and the destination"));
    }
    let _state = state_root(cli.state.as_deref())?;

    println!("root {}", root.display());
    println!("plan {src}#{item} -> {dest}");
    println!("next: the plan, the apply and the drag fixpoint land in this verb");
    Ok(())
}

/// `<SRC>#<ITEM>` split at the last `#`, so a path holding one still parses.
fn split_target(target: &str) -> Result<(PathBuf, String), String> {
    let (src, item) = target
        .rsplit_once('#')
        .ok_or_else(|| format!("a cleave target is `<SRC>#<ITEM>`, not {target}"))?;
    if src.is_empty() || item.is_empty() {
        return Err(format!("a cleave target is `<SRC>#<ITEM>`, not {target}"));
    }
    Ok((PathBuf::from(src), item.to_string()))
}

/// The corpus root: as asked, else the git root holding SRC.
fn plan_root(requested: Option<&PathBuf>, src: &Path) -> Result<PathBuf, String> {
    let root = match requested {
        Some(root) => {
            let root = absolute(root)?;
            if !root.is_dir() {
                return Err(format!("--root is not a directory: {}", root.display()));
            }
            root
        }
        None => {
            let src = anchor_file(src)?;
            let parent = src.parent().unwrap_or(&src).to_path_buf();
            soopy::discover(&parent)
                .map_err(|error| format!("discover root for {}: {error}", src.display()))?
                .root
        }
    };
    root.canonicalize()
        .map_err(|error| format!("canonicalize root {}: {error}", root.display()))
}

fn anchor_file(path: &Path) -> Result<PathBuf, String> {
    let path = absolute(path)?;
    if !path.is_file() {
        return Err(format!("cleave source is not a file: {}", path.display()));
    }
    path.canonicalize()
        .map_err(|error| format!("canonicalize {}: {error}", path.display()))
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Ok(normalize(path));
    }
    let cwd = std::env::current_dir().map_err(|error| format!("current directory: {error}"))?;
    Ok(normalize(&cwd.join(path)))
}

/// DEST need not exist yet, so only its deepest existing ancestor canonicalizes;
/// the tail is re-appended so root-relative stripping still holds.
fn canonical_unborn(path: &Path) -> PathBuf {
    let path = normalize(path);
    let mut tail = Vec::new();
    let mut probe = path.as_path();
    loop {
        if let Ok(real) = probe.canonicalize() {
            let mut out = real;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return out;
        }
        let (Some(parent), Some(name)) = (probe.parent(), probe.file_name()) else {
            return path;
        };
        tail.push(name.to_os_string());
        probe = parent;
    }
}

fn within_root(root: &Path, path: &Path) -> Result<String, String> {
    path.strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .map_err(|_| format!("{} is outside root {}", path.display(), root.display()))
}
