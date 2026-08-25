//! Lane names derived from one branch name: `feature/schema-emit` is the lane
//! id, the tmux session and the worktree path. `--lane <id>` still spawns.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::config;
use boop_harness::identity::Identity;
use boop_harness::registry::Registry;
use boop_harness::{HarnessId, LanePolicy};
use boop_store::bus::Route;

/// The worktree parent directory, relative to the repo root.
pub const WORKTREE_ROOT: &str = ".boop-worktrees";

/// Conventional prefixes; nothing here rejects another one.
pub const KINDS: [&str; 4] = ["feature", "fix", "refactor", "chore"];

/// The harness a spawn runs on when nothing names one: the lane default.
const DEFAULT_SPAWN_HARNESS: HarnessId = HarnessId::Opencode;

/// Model-family prefix -> owning harness on a flat-rate plan. The default ban
/// table; config `opencode-banned` wins when set.
const PLAN_FAMILY_TO_HARNESS: [(&str, &str); 10] = [
    ("gpt", "codex"),
    ("codex", "codex"),
    ("o3", "codex"),
    ("o4", "codex"),
    ("claude", "claude"),
    ("opus", "claude"),
    ("sonnet", "claude"),
    ("haiku", "claude"),
    ("fable", "claude"),
    ("gemini", "gemini"),
];

/// Every name one spawn answers to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaneIdentity {
    pub lane: String,
    pub tmux: String,
    pub branch: String,
    /// `None` runs the spawn in the repo itself (the pre-branch lane shape).
    pub worktree_dir: Option<PathBuf>,
}

/// A base commit and the rev that produced it, so a dry-run prints both.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseSha {
    pub sha: String,
    pub rev: String,
}

/// A parent lane and the rung that produced it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParentPick {
    pub parent: Option<String>,
    pub source: &'static str,
}

/// The kind prefix of a branch, when it is one of `KINDS`.
pub fn kind_of(branch: &str) -> Option<&'static str> {
    let (prefix, rest) = branch.split_once('/')?;
    if rest.is_empty() {
        return None;
    }
    KINDS.into_iter().find(|kind| *kind == prefix)
}

/// A branch name as one shell word. tmux rejects `.` and `:` in a session name,
/// so every byte outside `[A-Za-z0-9_-]` collapses into a single `-`.
pub fn slug(branch: &str) -> String {
    let mut slugged = String::with_capacity(branch.len());
    for character in branch.chars() {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
            slugged.push(character);
        } else if !slugged.ends_with('-') {
            slugged.push('-');
        }
    }
    slugged.trim_matches('-').to_owned()
}

/// The directory a branch's worktree lives in; slashes stay slashes, which is
/// the path the `lane/*` worktrees already on disk have.
pub fn worktree_dir(repo: &Path, branch: &str) -> PathBuf {
    let mut path = repo.join(WORKTREE_ROOT);
    for segment in branch.split('/').filter(|segment| !segment.is_empty()) {
        path.push(segment);
    }
    path
}

/// Resolve one spawn's names. A branch means a worktree; without one the caller
/// names the lane and the spawn runs in the repo.
pub fn derive(
    repo: &Path,
    branch: Option<&str>,
    lane_override: Option<&str>,
    tmux_override: Option<&str>,
) -> Result<LaneIdentity> {
    let Some(branch) = branch else {
        let Some(lane) = lane_override.filter(|lane| !lane.is_empty()) else {
            anyhow::bail!(
                "name the lane: --branch feature/<name> (also fix/, refactor/, chore/), or the older --lane <id>"
            );
        };
        return Ok(LaneIdentity {
            tmux: tmux_override.unwrap_or(lane).to_owned(),
            branch: tmux_override.unwrap_or(lane).to_owned(),
            lane: lane.to_owned(),
            worktree_dir: None,
        });
    };
    check_branch(branch)?;
    let lane = match lane_override.filter(|lane| !lane.is_empty()) {
        Some(lane) => lane.to_owned(),
        None => slug(branch),
    };
    if lane.is_empty() {
        anyhow::bail!("branch `{branch}` has no name left after slugging; pass --lane <id>");
    }
    Ok(LaneIdentity {
        tmux: tmux_override.unwrap_or(&lane).to_owned(),
        lane,
        branch: branch.to_owned(),
        worktree_dir: Some(worktree_dir(repo, branch)),
    })
}

/// A branch name that also has to be a path segment under `.boop-worktrees`.
fn check_branch(branch: &str) -> Result<()> {
    if branch.is_empty() {
        anyhow::bail!("--branch is empty");
    }
    if branch.starts_with('/') || branch.ends_with('/') {
        anyhow::bail!("--branch `{branch}` cannot start or end with `/`");
    }
    if branch.split('/').any(|segment| segment.is_empty()) {
        anyhow::bail!("--branch `{branch}` has an empty path segment");
    }
    if branch.contains("..") {
        anyhow::bail!("--branch `{branch}` cannot contain `..`");
    }
    if branch
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        anyhow::bail!("--branch `{branch}` cannot contain whitespace");
    }
    Ok(())
}

/// The repo a spawn defaults to. Inside a linked worktree that is the repo
/// owning it, so `.boop-worktrees` never nests inside a worktree.
pub fn repo_root(from: &Path) -> Result<PathBuf> {
    let toplevel = git_line(from, &["rev-parse", "--show-toplevel"])
        .with_context(|| format!("no git repo at {}; pass --cwd <repo>", from.display()))?;
    let common = git_line(
        from,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    );
    if let Some(common) = common.map(PathBuf::from) {
        if common.file_name().and_then(|name| name.to_str()) == Some(".git") {
            if let Some(parent) = common.parent() {
                return Ok(parent.to_path_buf());
            }
        }
    }
    Ok(PathBuf::from(toplevel))
}

/// The commit a lane branches from when the caller pinned none. Local refs
/// only; a stale `origin/main` is the caller's fetch to do.
pub fn default_base_sha(repo: &Path) -> Result<BaseSha> {
    for rev in ["origin/main", "origin/HEAD", "HEAD"] {
        if let Some(sha) = rev_parse(repo, rev) {
            return Ok(BaseSha {
                sha,
                rev: rev.to_owned(),
            });
        }
    }
    anyhow::bail!(
        "no base commit in {}: origin/main, origin/HEAD and HEAD all fail to resolve; pass --base-sha",
        repo.display()
    )
}

/// The commit a rev names, or `None` when the rev resolves to no commit.
pub fn rev_parse(repo: &Path, rev: &str) -> Option<String> {
    git_line(
        repo,
        &["rev-parse", "--verify", "-q", &format!("{rev}^{{commit}}")],
    )
}

fn git_line(repo: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!line.is_empty()).then_some(line)
}

/// The UTF-8 locale a spawn falls back to when the caller has none. macOS
/// ships no `C.UTF-8`; minimal Linux images ship no `en_US.UTF-8`.
pub const FALLBACK_LOCALE: &str = if cfg!(target_os = "macos") {
    "en_US.UTF-8"
} else {
    "C.UTF-8"
};

/// `LC_ALL` and `LANG` for a spawn, as a shell prefix. A tmux server started
/// outside a login shell hands its sessions a bare `C`, failing UTF-8 gates.
pub fn locale_stamp() -> String {
    let locale = shell_word(&utf8_locale());
    format!("LC_ALL={locale} LANG={locale}")
}

/// The caller's own locale when it is UTF-8, else `FALLBACK_LOCALE`.
pub fn utf8_locale() -> String {
    locale_from(
        std::env::var("LC_ALL").ok().as_deref(),
        std::env::var("LANG").ok().as_deref(),
    )
}

fn locale_from(lc_all: Option<&str>, lang: Option<&str>) -> String {
    [lc_all, lang]
        .into_iter()
        .flatten()
        .find(|value| is_utf8(value))
        .unwrap_or(FALLBACK_LOCALE)
        .to_owned()
}

fn is_utf8(value: &str) -> bool {
    value
        .to_ascii_lowercase()
        .replace('-', "")
        .ends_with("utf8")
}

fn shell_word(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

pub use boop_harness::worktree::{
    brief_with_preamble, record_start_status, start_preamble, start_status_path, SETUP_SENTENCE,
};

/// The shell line a lane pane runs once its supervisor has exited: the route
/// drop only. The completion row is `supervise::record_result`'s to write.
pub fn pane_epilogue(lane: &str, mail_dir: &Path) -> String {
    format!(
        "boop beep lane delete {} --route-only --mail-dir {}",
        shell_word(lane),
        shell_word(&mail_dir.display().to_string()),
    )
}

pub use boop_store::session::{Effort, ModelSpec};

/// The harness a model spelling names, or `None` when it names none. Config's
/// `model-harness` wins for a bare name; otherwise `HarnessId::for_model` does.
pub fn harness_for_model(model: &str) -> Result<Option<HarnessId>> {
    let spec: ModelSpec = model.parse()?;
    let name = spec.name.trim();
    if name.is_empty() || name.contains('/') {
        return Ok(HarnessId::for_model(model));
    }
    let lowered = name.to_ascii_lowercase();
    let config = config::loaded()?;
    let configured = config
        .model_harness
        .iter()
        .find(|(prefix, _)| lowered.starts_with(prefix.as_str()))
        .and_then(|(_, harness)| HarnessId::parse(harness));
    Ok(configured.or_else(|| HarnessId::for_model(model)))
}

/// A model family whose own harness runs on a flat-rate plan; opencode would
/// pay metered credit for it, so spawn refuses with no override.
fn plan_harness_family(model: &str) -> Result<Option<Cow<'static, str>>> {
    let spec: ModelSpec = model.parse()?;
    let lowered = spec.name.to_ascii_lowercase();
    let name = lowered.trim();
    let bare = name.rsplit('/').next().unwrap_or(name);
    let config = config::loaded()?;
    Ok(config
        .opencode_banned
        .iter()
        .find(|(prefix, _)| bare.starts_with(prefix.as_str()))
        .map(|(_, owner)| Cow::Borrowed(owner.as_str()))
        .or_else(|| {
            PLAN_FAMILY_TO_HARNESS
                .into_iter()
                .find(|(prefix, _)| bare.starts_with(prefix))
                .map(|(_, owner)| Cow::Borrowed(owner))
        }))
}

/// The refused-from-opencode error, naming the flat-rate harness that owns the
/// family.
fn banned_error(model: &str, owner: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "model `{model}` is BANNED from opencode: its family runs on the `{owner}` harness's flat-rate plan, and opencode would pay metered API credit for it. Spell the bare model name (no provider path) so the `{owner}` harness picks it up."
    )
}

/// The harness a spawn runs on: `--harness` wins, else the model spelling.
/// Both refusals below read the harness's own declared capabilities.
pub fn harness_for_spawn(
    registry: &Registry,
    explicit: Option<&str>,
    model: Option<&str>,
) -> Result<HarnessId> {
    let model_named = model.filter(|model| !model.is_empty());
    let explicit = explicit.filter(|harness| !harness.is_empty());
    let harness = match (explicit, model_named) {
        (Some(explicit), _) => explicit.parse::<HarnessId>()?,
        (None, None) => DEFAULT_SPAWN_HARNESS,
        (None, Some(model)) => harness_for_model(model)?.with_context(|| {
            format!(
                "model `{model}` names no harness (gpt-* codex, kimi-* kimi, claude-* claude, provider/model opencode); pass --harness <id>"
            )
        })?,
    };
    let capabilities = registry.get(harness).capabilities();
    if capabilities.bans_plan_family_models {
        if let Some(model) = model_named {
            if let Some(owner) = plan_harness_family(model)? {
                return Err(banned_error(model, &owner));
            }
        }
    }
    if explicit.is_none() && capabilities.lanes == LanePolicy::CoordinatorSubagentsOnly {
        let model = model_named.unwrap_or_default();
        anyhow::bail!(
            "model `{model}` runs on the `{harness}` harness, whose workers are the coordinator's own Agent-tool subagents, never tmux lanes; pass --harness {harness} to spawn one anyway"
        );
    }
    Ok(harness)
}

/// The caller's own registered route. The identity ladder names a lane
/// outright; without one the caller's session must match exactly one route,
/// because two matches name two different senders.
pub fn caller_route<'a>(
    identity: &Identity,
    routes: &'a BTreeMap<String, Route>,
) -> Result<(String, &'a Route)> {
    if let Some(lane) = identity.lane.as_deref().filter(|lane| !lane.is_empty()) {
        let route = routes.get(lane).with_context(|| {
            format!(
                "unknown caller: identity names lane `{lane}`, which the registry does not carry"
            )
        })?;
        return Ok((lane.to_owned(), route));
    }
    let session = identity
        .session
        .as_deref()
        .filter(|session| !session.is_empty())
        .context(
            "unknown caller: no lane and no session resolved (boop whoami shows the ladder)",
        )?;
    let mut hits = routes
        .iter()
        .filter(|(_, route)| route.session_id.as_deref() == Some(session));
    match (hits.next(), hits.next()) {
        (Some((name, route)), None) => Ok((name.clone(), route)),
        (Some((first, _)), Some((second, _))) => anyhow::bail!(
            "ambiguous caller: session `{session}` is registered as both `{first}` and `{second}`"
        ),
        (None, _) => {
            anyhow::bail!("unknown caller: no registered route carries session `{session}`")
        }
    }
}

/// Where a child's own mail goes: the parent edge its registration recorded,
/// else the one registered coordinator. A caller that would address itself has
/// no parent.
pub fn tell_parent_target(
    caller: &str,
    route: &Route,
    routes: &BTreeMap<String, Route>,
    stamped: Option<&str>,
) -> Result<ParentPick> {
    if let Some(parent) = route.parent.as_deref().filter(|parent| !parent.is_empty()) {
        return Ok(ParentPick {
            parent: Some(parent.to_owned()),
            source: "edge",
        });
    }
    // A process the spawner stamped knows its own parent even when the
    // registry row does not carry the edge, which is every adopted pane.
    if let Some(stamped) = stamped.filter(|parent| !parent.is_empty() && *parent != caller) {
        return Ok(ParentPick {
            parent: Some(stamped.to_owned()),
            source: "stamp",
        });
    }
    let pick = resolve_parent(None, None, routes);
    match pick.parent.as_deref() {
        Some(parent) if parent != caller => Ok(pick),
        _ => anyhow::bail!(
            "no parent edge: `{caller}` registered no parent and the registry holds no single other coordinator to fall back to; respawn with `--parent <route>` or register one"
        ),
    }
}

/// Every route the registry records as a child of `parent`.
pub fn children_of<'a>(
    parent: &str,
    routes: &'a BTreeMap<String, Route>,
) -> Vec<(&'a str, &'a Route)> {
    routes
        .iter()
        .filter(|(name, route)| route.parent.as_deref() == Some(parent) && name.as_str() != parent)
        .map(|(name, route)| (name.as_str(), route))
        .collect()
}

/// What a `yield` says when the caller names no body: the lane, a clean exit,
/// and the branch and head of the tree it worked in.
pub fn yield_body(lane: &str, tree: Option<&Path>) -> String {
    let branch = tree
        .and_then(|tree| git_line(tree, &["rev-parse", "--abbrev-ref", "HEAD"]))
        .unwrap_or_else(|| "-".to_owned());
    let head = tree
        .and_then(|tree| rev_parse(tree, "HEAD"))
        .unwrap_or_else(|| "-".to_owned());
    format!("yield {lane} rc=0 branch={branch} head={head}")
}

/// Who gets the completion hail: the flag, else the caller (a spawner is the
/// parent of what it spawns), else a lone pane-backed registered coordinator.
pub fn resolve_parent(
    explicit: Option<&str>,
    caller_lane: Option<&str>,
    routes: &BTreeMap<String, Route>,
) -> ParentPick {
    if let Some(parent) = explicit.filter(|parent| !parent.is_empty()) {
        return ParentPick {
            parent: Some(parent.to_owned()),
            source: "flag",
        };
    }
    if let Some(lane) = caller_lane.filter(|lane| !lane.is_empty()) {
        return ParentPick {
            parent: Some(lane.to_owned()),
            source: "caller",
        };
    }
    let mut coordinators = routes
        .iter()
        .filter(|(_, route)| {
            route.kind == "coordinator"
                && route
                    .tmux
                    .as_deref()
                    .is_some_and(|target| !target.is_empty())
        })
        .map(|(lane, _)| lane);
    let (Some(only), None) = (coordinators.next(), coordinators.next()) else {
        return ParentPick {
            parent: None,
            source: "none",
        };
    };
    ParentPick {
        parent: Some(only.clone()),
        source: "registry",
    }
}

/// A worktree and branch a lane left behind after its registry route was
/// already torn down.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaneCarcass {
    pub worktree: PathBuf,
    pub branch: String,
}

/// The carcass a lane id names. A carcass carries no route, so the repo's own
/// worktree list is where its branch and path come from.
pub fn find_carcass(repo: &Path, lane: &str) -> Option<LaneCarcass> {
    let listing = git_line(repo, &["worktree", "list", "--porcelain"])?;
    carcass_in_listing(repo, &listing, lane)
}

/// The `git worktree list --porcelain` entry whose branch slugs to `lane`, or
/// whose directory is named `lane`. The repo's own worktree never matches.
fn carcass_in_listing(repo: &Path, listing: &str, lane: &str) -> Option<LaneCarcass> {
    let mut path: Option<PathBuf> = None;
    for line in listing.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            path = Some(PathBuf::from(rest));
            continue;
        }
        let Some(branch) = line.strip_prefix("branch refs/heads/") else {
            continue;
        };
        let Some(worktree) = path.clone() else {
            continue;
        };
        if worktree == repo {
            continue;
        }
        let named = worktree
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == lane);
        if slug(branch) == lane || named {
            return Some(LaneCarcass {
                worktree,
                branch: branch.to_owned(),
            });
        }
    }
    None
}

/// `lane delete` on a lane whose route is gone: the DOA case, where the on-exit
/// epilogue dropped the route and left the worktree and branch standing.
pub fn delete_carcass(
    repo: &Path,
    lane: &str,
    pane_alive: impl Fn(&str) -> bool,
) -> Result<boop_harness::worktree::Reclaimed> {
    let Some(carcass) = find_carcass(repo, lane) else {
        anyhow::bail!(
            "no registry route for lane `{lane}`, and no worktree under {} answers to it",
            repo.display()
        )
    };
    if pane_alive(lane) {
        anyhow::bail!(
            "lane `{lane}` has no route but its tmux session is alive; \
             `boop beep lane patch` re-routes it, delete takes dead lanes only"
        );
    }
    boop_harness::worktree::reclaim_carcass(repo, &carcass.branch, &carcass.worktree)
}

/// `lane create --reclaim`: clear a dead lane's worktree and branch before the
/// spawn. A live route or a live pane refuses.
pub fn reclaim_for_spawn(
    repo: &Path,
    identity: &LaneIdentity,
    routes: &BTreeMap<String, Route>,
    pane_alive: impl Fn(&str) -> bool,
) -> Result<boop_harness::worktree::Reclaimed> {
    let Some(worktree) = identity.worktree_dir.as_deref() else {
        anyhow::bail!("--reclaim needs a worktree spawn; name one with --branch");
    };
    // A tmux target that is gone has no pane pid left to read, so one liveness
    // question answers both.
    let target = routes
        .get(&identity.lane)
        .and_then(|route| route.tmux.clone())
        .unwrap_or_else(|| identity.tmux.clone());
    if pane_alive(&target) {
        anyhow::bail!(
            "lane `{}` is live on tmux target {target}; --reclaim takes dead lanes only",
            identity.lane
        );
    }
    boop_harness::worktree::reclaim_carcass(repo, &identity.branch, worktree)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use boop_store::bus::Route;

    use super::{
        caller_route, children_of, default_base_sha, derive, harness_for_model, harness_for_spawn,
        kind_of, repo_root, resolve_parent, rev_parse, slug, tell_parent_target, yield_body,
        HarnessId, Identity, LaneIdentity, ModelSpec, Registry, FALLBACK_LOCALE,
    };

    fn repo() -> PathBuf {
        PathBuf::from("/repo")
    }

    /// RECEIPT. Each conventional kind names a lane the same way, with no
    /// `lane/` prefix anywhere and one slug for both lane id and tmux.
    #[test]
    fn each_conventional_kind_derives_the_same_shape() {
        for kind in super::KINDS {
            let branch = format!("{kind}/schema-emit");
            let identity = derive(&repo(), Some(&branch), None, None).unwrap();
            assert_eq!(
                identity,
                LaneIdentity {
                    lane: format!("{kind}-schema-emit"),
                    tmux: format!("{kind}-schema-emit"),
                    branch: branch.clone(),
                    worktree_dir: Some(
                        PathBuf::from("/repo/.boop-worktrees")
                            .join(kind)
                            .join("schema-emit")
                    ),
                },
                "branch {branch}"
            );
            assert_eq!(kind_of(&branch), Some(kind));
        }
    }

    /// RECEIPT. A slash is spelled `-` in the lane id and tmux session, as is
    /// every character tmux rejects.
    #[test]
    fn slugging_spells_a_slash_as_a_dash() {
        assert_eq!(slug("feature/schema-emit"), "feature-schema-emit");
        assert_eq!(slug("fix/v6.2/parse"), "fix-v6-2-parse");
        assert_eq!(slug("chore/a:b"), "chore-a-b");
        assert_eq!(slug("refactor//double//slash"), "refactor-double-slash");
        assert_eq!(slug("/leading/and/trailing/"), "leading-and-trailing");
        assert_eq!(slug("keeps_underscores"), "keeps_underscores");
    }

    /// RECEIPT. The prefix is a convention, never a gate: an unconventional
    /// branch still spawns and reports no kind.
    #[test]
    fn an_unconventional_prefix_still_derives() {
        let identity = derive(&repo(), Some("lane/boop-sql"), None, None).unwrap();
        assert_eq!(identity.lane, "lane-boop-sql");
        assert_eq!(
            identity.worktree_dir,
            Some(PathBuf::from("/repo/.boop-worktrees/lane/boop-sql")),
            "the worktrees already on disk keep their path"
        );
        assert_eq!(kind_of("lane/boop-sql"), None);
        assert_eq!(kind_of("feature"), None, "a bare kind names nothing");
        assert_eq!(kind_of("feature/"), None);
    }

    /// RECEIPT. The pre-branch shape (`--lane <id>` alone) keeps its exact
    /// behavior: no worktree, branch equal to the lane id.
    #[test]
    fn the_older_lane_shape_still_derives_without_a_worktree() {
        let identity = derive(&repo(), None, Some("boop-sql"), None).unwrap();
        assert_eq!(
            identity,
            LaneIdentity {
                lane: "boop-sql".into(),
                tmux: "boop-sql".into(),
                branch: "boop-sql".into(),
                worktree_dir: None,
            }
        );
        let with_tmux = derive(&repo(), None, Some("boop-sql"), Some("pane-7")).unwrap();
        assert_eq!(with_tmux.tmux, "pane-7");
        assert_eq!(with_tmux.branch, "pane-7", "the pre-branch fallback order");
    }

    /// RECEIPT. A lane id already in the registry respawns under a new branch
    /// without renaming the row.
    #[test]
    fn overrides_win_over_the_derived_names() {
        let identity = derive(
            &repo(),
            Some("feature/schema-emit"),
            Some("schema-emit"),
            Some("old-pane"),
        )
        .unwrap();
        assert_eq!(identity.lane, "schema-emit");
        assert_eq!(identity.tmux, "old-pane");
        assert_eq!(identity.branch, "feature/schema-emit");
    }

    #[test]
    fn a_nameless_spawn_is_an_error_naming_the_flag() {
        let error = derive(&repo(), None, None, None).unwrap_err().to_string();
        assert!(error.contains("--branch feature/"), "message: {error}");
        assert!(error.contains("--lane"), "message: {error}");
    }

    /// RECEIPT. A branch that would escape `.boop-worktrees` is refused before
    /// git or the filesystem sees it.
    #[test]
    fn a_branch_that_escapes_the_worktree_root_is_refused() {
        for bad in ["../../etc", "/absolute", "trailing/", "has space", "a//b"] {
            assert!(
                derive(&repo(), Some(bad), None, None).is_err(),
                "branch {bad} must be refused"
            );
        }
    }

    fn git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn seed_repo(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("boop-lane-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Command::new("git")
            .args(["init", "-q"])
            .arg(&path)
            .status()
            .unwrap();
        git(&path, &["config", "user.email", "t@t"]);
        git(&path, &["config", "user.name", "t"]);
        std::fs::write(path.join("seed.txt"), "seed").unwrap();
        git(&path, &["add", "-A"]);
        git(&path, &["commit", "-qm", "seed"]);
        path
    }

    /// RECEIPT. The default base is `origin/main`, read from local refs: the
    /// fixture writes the remote ref by hand, so no test touches the network.
    #[test]
    fn the_default_base_sha_prefers_origin_main() {
        let path = seed_repo("base");
        let head = rev_parse(&path, "HEAD").unwrap();
        assert_eq!(
            default_base_sha(&path).unwrap().rev,
            "HEAD",
            "with no remote ref the fallback is the local head"
        );
        std::fs::write(path.join("next.txt"), "next").unwrap();
        git(&path, &["add", "-A"]);
        git(&path, &["commit", "-qm", "next"]);
        let moved_head = rev_parse(&path, "HEAD").unwrap();
        git(&path, &["update-ref", "refs/remotes/origin/main", &head]);
        let base = default_base_sha(&path).unwrap();
        assert_eq!(base.rev, "origin/main");
        assert_eq!(base.sha, head, "origin/main wins over the local head");
        assert_ne!(base.sha, moved_head);
        let _ = std::fs::remove_dir_all(&path);
    }

    /// RECEIPT. Without origin/main, origin/HEAD still beats the local head.
    #[test]
    fn the_default_base_sha_falls_back_to_origin_head() {
        let path = seed_repo("originhead");
        let head = rev_parse(&path, "HEAD").unwrap();
        git(&path, &["update-ref", "refs/remotes/origin/trunk", &head]);
        git(
            &path,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/trunk",
            ],
        );
        std::fs::write(path.join("next.txt"), "next").unwrap();
        git(&path, &["add", "-A"]);
        git(&path, &["commit", "-qm", "next"]);
        let base = default_base_sha(&path).unwrap();
        assert_eq!(base.rev, "origin/HEAD");
        assert_eq!(base.sha, head);
        let _ = std::fs::remove_dir_all(&path);
    }

    /// RECEIPT. A spawn issued from inside a worktree branches the repo that
    /// owns it, so worktrees never nest.
    #[test]
    fn the_default_repo_from_a_worktree_is_the_owning_repo() {
        let path = seed_repo("root");
        let nested = path.join("sub");
        std::fs::create_dir_all(&nested).unwrap();
        assert_eq!(
            std::fs::canonicalize(repo_root(&nested).unwrap()).unwrap(),
            std::fs::canonicalize(&path).unwrap()
        );
        let linked = path.join(".boop-worktrees/feature/x");
        git(
            &path,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "feature/x",
                &linked.display().to_string(),
            ],
        );
        assert_eq!(
            std::fs::canonicalize(repo_root(&linked).unwrap()).unwrap(),
            std::fs::canonicalize(&path).unwrap(),
            "a linked worktree resolves to the repo that owns it"
        );
        let _ = std::fs::remove_dir_all(&path);
    }

    fn route(tmux: &str) -> Route {
        route_of_kind("lane", tmux)
    }

    fn route_of_kind(kind: &str, tmux: &str) -> Route {
        Route {
            kind: kind.into(),
            harness: Some(HarnessId::Opencode),
            tmux: Some(tmux.into()),
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
            app_server_socket: None,
        }
    }

    /// RECEIPT. The parent ladder stops at the first rung that answers: flag,
    /// caller, lone coordinator.
    #[test]
    fn the_parent_ladder_stops_at_the_first_rung_that_answers() {
        let mut routes = BTreeMap::new();
        routes.insert(
            "sprefa-coordinator".to_owned(),
            route_of_kind("coordinator", "shell:0.0"),
        );
        routes.insert("boop-sql".to_owned(), route("boop-sql"));

        let flagged = resolve_parent(Some("named"), Some("caller"), &routes);
        assert_eq!(flagged.parent.as_deref(), Some("named"));
        assert_eq!(flagged.source, "flag");

        let from_caller = resolve_parent(None, Some("caller"), &routes);
        assert_eq!(from_caller.parent.as_deref(), Some("caller"));
        assert_eq!(from_caller.source, "caller");

        let from_registry = resolve_parent(None, None, &routes);
        assert_eq!(from_registry.parent.as_deref(), Some("sprefa-coordinator"));
        assert_eq!(from_registry.source, "registry");
    }

    /// RECEIPT. A lane named `coordinator-*` but not kind=coordinator is
    /// never picked; the name substring named nothing.
    #[test]
    fn a_lane_named_coordinator_but_not_kind_coordinator_is_not_selected() {
        let mut routes = BTreeMap::new();
        routes.insert(
            "coordinator-imposter".to_owned(),
            route_of_kind("lane", "shell:0.1"),
        );
        let pick = resolve_parent(None, None, &routes);
        assert_eq!(pick.parent, None);
        assert_eq!(pick.source, "none");
    }

    /// RECEIPT. A kind=coordinator route named without the word is still the
    /// lone coordinator picked.
    #[test]
    fn a_kind_coordinator_route_named_without_the_word_is_selected() {
        let mut routes = BTreeMap::new();
        routes.insert(
            "terra".to_owned(),
            route_of_kind("coordinator", "shell:0.2"),
        );
        routes.insert("boop-sql".to_owned(), route("boop-sql"));
        let pick = resolve_parent(None, None, &routes);
        assert_eq!(pick.parent.as_deref(), Some("terra"));
        assert_eq!(pick.source, "registry");
    }

    fn identity_of(lane: Option<&str>, session: Option<&str>) -> Identity {
        Identity {
            session: session.map(str::to_owned),
            lane: lane.map(str::to_owned),
            ..Identity::default()
        }
    }

    /// RECEIPT. The ladder's lane wins outright, and a lane the registry does
    /// not carry is named in the error rather than treated as unregistered
    /// mail with no sender.
    #[test]
    fn the_caller_is_the_lane_the_ladder_names() {
        let mut routes = BTreeMap::new();
        routes.insert("boop-sql".to_owned(), route("boop-sql"));

        let (name, _) = caller_route(&identity_of(Some("boop-sql"), None), &routes).unwrap();
        assert_eq!(name, "boop-sql");

        let error = caller_route(&identity_of(Some("ghost"), None), &routes).unwrap_err();
        assert!(
            error.to_string().contains("unknown caller") && error.to_string().contains("ghost"),
            "{error}"
        );
    }

    /// RECEIPT. Two routes on one session name two senders, so the caller is
    /// reported ambiguous instead of picking the first key in the map.
    #[test]
    fn a_session_on_two_routes_is_an_ambiguous_caller() {
        let mut routes = BTreeMap::new();
        let mut first = route("one");
        first.session_id = Some("ses-7".to_owned());
        let mut second = route("two");
        second.session_id = Some("ses-7".to_owned());
        routes.insert("lane-one".to_owned(), first);
        routes.insert("lane-two".to_owned(), second);

        let error = caller_route(&identity_of(None, Some("ses-7")), &routes).unwrap_err();
        assert!(error.to_string().contains("ambiguous caller"), "{error}");

        routes.remove("lane-two");
        let (name, _) = caller_route(&identity_of(None, Some("ses-7")), &routes).unwrap();
        assert_eq!(name, "lane-one");
    }

    /// RECEIPT. The recorded edge outranks the registry rung, and a caller
    /// that is itself the lone coordinator has no parent to tell.
    #[test]
    fn the_recorded_edge_outranks_the_lone_coordinator() {
        let mut routes = BTreeMap::new();
        routes.insert(
            "terra".to_owned(),
            route_of_kind("coordinator", "shell:0.3"),
        );
        let mut child = route("boop-sql");
        child.parent = Some("luna".to_owned());
        routes.insert("boop-sql".to_owned(), child.clone());

        let edge = tell_parent_target("boop-sql", &child, &routes, None).unwrap();
        assert_eq!(edge.parent.as_deref(), Some("luna"));
        assert_eq!(edge.source, "edge");

        let mut orphan = route("boop-sql");
        orphan.parent = None;
        let fallback = tell_parent_target("boop-sql", &orphan, &routes, None).unwrap();
        assert_eq!(fallback.parent.as_deref(), Some("terra"));
        assert_eq!(fallback.source, "registry");

        let coordinator = routes.get("terra").unwrap().clone();
        let error = tell_parent_target("terra", &coordinator, &routes, None).unwrap_err();
        assert!(error.to_string().contains("no parent edge"), "{error}");

        // The spawn stamp names a parent the registry row never carried, and
        // a stamp naming the caller itself is not a parent edge.
        let stamped = tell_parent_target("terra", &coordinator, &routes, Some("claude-5")).unwrap();
        assert_eq!(stamped.parent.as_deref(), Some("claude-5"));
        assert_eq!(stamped.source, "stamp");
        assert!(tell_parent_target("terra", &coordinator, &routes, Some("terra")).is_err());
    }

    /// RECEIPT. Children are the recorded edges pointing at the caller, and a
    /// route naming itself as its own parent is not one of them.
    #[test]
    fn children_are_the_routes_recording_the_caller_as_parent() {
        let mut routes = BTreeMap::new();
        let mut child = route("child");
        child.parent = Some("terra".to_owned());
        let mut stranger = route("stranger");
        stranger.parent = Some("luna".to_owned());
        let mut looped = route_of_kind("coordinator", "shell:0.4");
        looped.parent = Some("terra".to_owned());
        routes.insert("boop-sql".to_owned(), child);
        routes.insert("other-lane".to_owned(), stranger);
        routes.insert("terra".to_owned(), looped);

        let children: Vec<&str> = children_of("terra", &routes)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert_eq!(children, vec!["boop-sql"]);
    }

    /// RECEIPT. A yield from a route with no tree still says what it is; git
    /// answering nothing prints a dash, never an empty field.
    #[test]
    fn a_yield_body_with_no_tree_reads_as_dashes() {
        assert_eq!(
            yield_body("feature-a", None),
            "yield feature-a rc=0 branch=- head=-"
        );
        let body = yield_body("feature-a", Some(Path::new("/nonexistent-tree")));
        assert_eq!(body, "yield feature-a rc=0 branch=- head=-");
    }

    /// RECEIPT (field, 2026-08-10). `--model gpt-5.6-luna@medium` with no
    /// `--harness` dry-ran as opencode; the spelling names the harness now.
    #[test]
    fn a_gpt_model_names_the_codex_harness() {
        let registry = Registry::discover();
        assert_eq!(
            harness_for_model("gpt-5.6-luna@medium").unwrap(),
            Some(HarnessId::Codex)
        );
        assert_eq!(
            harness_for_spawn(&registry, None, Some("gpt-5.6-luna@medium")).unwrap(),
            HarnessId::Codex
        );
        assert_eq!(harness_for_model("kimi-k2").unwrap(), Some(HarnessId::Kimi));
        assert_eq!(
            harness_for_model("openrouter/deepseek/deepseek-v4-flash-0731").unwrap(),
            Some(HarnessId::Opencode)
        );
        assert_eq!(
            harness_for_model("zai-coding-plan/glm-4.6").unwrap(),
            Some(HarnessId::Opencode)
        );
        assert_eq!(harness_for_model("nothing-known").unwrap(), None);
    }

    /// RECEIPT (field, 2026-08-11). `openrouter/openai/gpt-5.6-sol` spawned
    /// two dead opencode lanes AND billed openrouter for plan-covered models.
    #[test]
    fn plan_family_models_are_banned_from_opencode() {
        let registry = Registry::discover();
        let err = harness_for_spawn(&registry, None, Some("openrouter/openai/gpt-5.6-sol"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("BANNED from opencode"), "{err}");
        assert!(err.contains("codex"), "{err}");
        let err = harness_for_spawn(&registry, Some("opencode"), Some("openai/gpt-5.6-terra"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("BANNED from opencode"), "{err}");
        let err = harness_for_spawn(&registry, None, Some("anthropic/claude-sonnet-5"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("claude"), "{err}");
        let err = harness_for_spawn(&registry, Some("opencode"), Some("google/gemini-3-pro"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("gemini"), "{err}");
        assert_eq!(
            harness_for_spawn(
                &registry,
                None,
                Some("openrouter/deepseek/deepseek-v4-flash-0731")
            )
            .unwrap(),
            HarnessId::Opencode
        );
        assert_eq!(
            harness_for_spawn(&registry, None, Some("zai-coding-plan/glm-4.6")).unwrap(),
            HarnessId::Opencode
        );
    }

    /// RECEIPT. An explicit --harness always wins, and no model at all keeps
    /// the opencode default the flash4 lanes run on.
    #[test]
    fn an_explicit_harness_wins_over_the_model_spelling() {
        let registry = Registry::discover();
        assert_eq!(
            harness_for_spawn(&registry, Some("kimi"), Some("gpt-5.6-luna")).unwrap(),
            HarnessId::Kimi
        );
        assert_eq!(
            harness_for_spawn(&registry, None, None).unwrap(),
            HarnessId::Opencode
        );
        assert_eq!(
            harness_for_spawn(&registry, Some("claude"), Some("claude-opus-4")).unwrap(),
            HarnessId::Claude,
            "the Agent-tool law is a default, and --harness claude is the way past it"
        );
    }

    /// RECEIPT. A Claude model without --harness stops, and an unknown
    /// spelling stops too rather than spawning on the wrong harness.
    #[test]
    fn an_unnamed_harness_never_guesses_opencode() {
        let registry = Registry::discover();
        let claude = harness_for_spawn(&registry, None, Some("claude-opus-4"))
            .unwrap_err()
            .to_string();
        assert!(claude.contains("Agent-tool"), "message: {claude}");
        let unknown = harness_for_spawn(&registry, None, Some("nothing-known"))
            .unwrap_err()
            .to_string();
        assert!(unknown.contains("--harness"), "message: {unknown}");
        assert!(unknown.contains("nothing-known"), "message: {unknown}");
    }

    /// RECEIPT (field). Two luna lanes failed SWI-Prolog UTF-8 gates that pass
    /// in a shell: a lane inherits the tmux server's bare locale, not a shell's.
    #[test]
    fn a_spawn_always_gets_a_utf8_locale() {
        assert_eq!(
            super::locale_from(Some("en_US.UTF-8"), None),
            "en_US.UTF-8",
            "the caller's own locale is inherited when it is UTF-8"
        );
        assert_eq!(
            super::locale_from(Some("C"), Some("en_GB.UTF-8")),
            "en_GB.UTF-8",
            "LC_ALL is read first, but a non-UTF-8 one never wins"
        );
        assert_eq!(
            super::locale_from(Some("C"), Some("POSIX")),
            FALLBACK_LOCALE
        );
        assert_eq!(super::locale_from(None, None), FALLBACK_LOCALE);
        assert!(
            FALLBACK_LOCALE.to_ascii_lowercase().contains("utf"),
            "the fallback has to be a UTF-8 locale: {FALLBACK_LOCALE}"
        );
        assert!(super::is_utf8("ja_JP.utf8"), "the dashless spelling counts");
    }

    /// RECEIPT. The stamp sets both variables, quoted, so a locale with no
    /// shell-safe spelling cannot split the command.
    #[test]
    fn the_locale_stamp_sets_lc_all_and_lang() {
        let stamp = super::locale_stamp();
        assert!(stamp.starts_with("LC_ALL='"), "stamp: {stamp}");
        assert!(stamp.contains(" LANG='"), "stamp: {stamp}");
        assert_eq!(
            stamp.matches("UTF-8'").count() + stamp.matches("utf8'").count(),
            2,
            "both variables carry the same UTF-8 locale: {stamp}"
        );
    }

    /// RECEIPT. Two coordinators are ambiguous, so nothing is guessed.
    #[test]
    fn two_coordinators_default_to_no_parent() {
        let mut routes = BTreeMap::new();
        routes.insert(
            "sprefa-coordinator".to_owned(),
            route_of_kind("coordinator", "a"),
        );
        routes.insert(
            "instant-coordinator".to_owned(),
            route_of_kind("coordinator", "b"),
        );
        let pick = resolve_parent(None, None, &routes);
        assert_eq!(pick.parent, None);
        assert_eq!(pick.source, "none");
        assert_eq!(resolve_parent(None, None, &BTreeMap::new()).source, "none");
    }

    /// RECEIPT. `ModelSpec` splits on the last `@`; a bad effort fails at
    /// parse naming the allowlist, not silently downstream as a literal model
    /// name.
    #[test]
    fn model_spec_splits_the_last_at_and_rejects_a_bad_effort() {
        let spec: ModelSpec = "gpt-5.6-luna@medium".parse().unwrap();
        assert_eq!(spec.name, "gpt-5.6-luna");
        assert_eq!(spec.effort, Some(super::Effort::Medium));

        let bare: ModelSpec = "gpt-5.6-sol".parse().unwrap();
        assert_eq!(bare.name, "gpt-5.6-sol");
        assert_eq!(bare.effort, None);

        let error = "x@turbo".parse::<ModelSpec>().unwrap_err().to_string();
        assert!(error.contains("low"), "message: {error}");
        assert!(error.contains("medium"), "message: {error}");
        assert!(error.contains("high"), "message: {error}");
    }

    /// RECEIPT. A lane id names its carcass through the branch slug, and the
    /// repo's own worktree never answers, whatever it is named.
    #[test]
    fn a_carcass_is_found_by_the_slug_of_its_branch() {
        let listing = concat!(
            "worktree /repo\nHEAD aaa\nbranch refs/heads/main\n\n",
            "worktree /repo/.boop-worktrees/feature/schema-emit\n",
            "HEAD bbb\nbranch refs/heads/feature/schema-emit\n"
        );
        let found = super::carcass_in_listing(&repo(), listing, "feature-schema-emit").unwrap();
        assert_eq!(
            found,
            super::LaneCarcass {
                worktree: PathBuf::from("/repo/.boop-worktrees/feature/schema-emit"),
                branch: "feature/schema-emit".to_owned(),
            }
        );
        assert_eq!(
            super::carcass_in_listing(&repo(), listing, "main"),
            None,
            "the repo's own worktree is never a carcass"
        );
        assert_eq!(
            super::carcass_in_listing(&repo(), listing, "feature-other"),
            None
        );
    }

    /// RECEIPT. `--reclaim` destroys git state, so a name whose pane still
    /// answers refuses before any git runs.
    #[test]
    fn a_reclaim_refuses_while_the_pane_is_alive() {
        let identity = derive(&repo(), Some("feature/schema-emit"), None, None).unwrap();
        let error = super::reclaim_for_spawn(&repo(), &identity, &BTreeMap::new(), |_| true)
            .unwrap_err()
            .to_string();
        assert!(error.contains("is live on tmux target"), "{error}");
        assert!(error.contains("dead lanes only"), "{error}");
    }

    /// A pane-less coordinator cannot receive the completion injection that
    /// makes it useful as an inferred parent.
    #[test]
    fn a_pane_less_coordinator_is_not_an_inferred_parent() {
        let mut routes = BTreeMap::new();
        let mut coordinator = route("unused");
        coordinator.kind = "coordinator".into();
        coordinator.tmux = None;
        routes.insert("sprefa-coordinator".to_owned(), coordinator);

        let pick = resolve_parent(None, None, &routes);
        assert_eq!(pick.parent, None);
        assert_eq!(pick.source, "none");
    }
}
