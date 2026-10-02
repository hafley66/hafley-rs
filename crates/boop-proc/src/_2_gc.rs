//! Owned disk candidates. Collection is read-only; application rechecks eligibility.
use anyhow::Result;
use boop_store::bus::{self, Route};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

pub const TARGET_AGE: Duration = Duration::from_secs(24 * 3600);
pub const DEAD_AGE: Duration = Duration::from_secs(7 * 24 * 3600);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Target,
    Trail,
    Worktree {
        repo: PathBuf,
        branch: String,
        base: String,
    },
}
#[derive(Clone, Debug)]
pub struct Candidate {
    pub path: PathBuf,
    pub bytes: u64,
    pub lane: String,
    pub state: &'static str,
    pub reason: &'static str,
    pub kind: Kind,
}

pub use boop_store::target_root::under;

/// Bytes and newest write anywhere in a tree, without following links. Read errors abstain.
pub fn tree(path: &Path) -> Result<(u64, SystemTime)> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    anyhow::ensure!(
        !name.starts_with("boop.db")
            && !matches!(
                name.as_ref(),
                "mail" | "mailboxes" | "bus.ndjson" | "registry.json"
            ),
        "protected store/mail path: {}",
        path.display()
    );
    let meta = fs::symlink_metadata(path)?;
    anyhow::ensure!(
        !meta.file_type().is_symlink(),
        "symlink: {}",
        path.display()
    );
    let mut newest = meta.modified()?;
    let mut bytes = if meta.is_file() { meta.len() } else { 0 };
    if meta.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            // A link itself is never traversed, including links to mail or the store.
            if entry.file_type()?.is_symlink() {
                continue;
            }
            let (size, touched) = tree(&entry.path())?;
            bytes += size;
            newest = newest.max(touched);
        }
    }
    Ok((bytes, newest))
}

pub fn old(now: SystemTime, touched: SystemTime, age: Duration) -> bool {
    now.duration_since(touched)
        .is_ok_and(|elapsed| elapsed >= age)
}

/// Activity includes registration and mail sent by the lane; incoming mail is no proof of life.
pub fn activity(
    mail: &Path,
    routes: &BTreeMap<String, Route>,
) -> Result<BTreeMap<String, SystemTime>> {
    let mut activity = BTreeMap::new();
    for (name, route) in routes {
        if let Some(stamp) = route.registered_at.as_deref().and_then(parse_time) {
            activity.insert(name.clone(), stamp);
        }
    }
    for message in bus::read_messages(mail)? {
        if let Some(stamp) = parse_time(&message.from_timestamp) {
            activity
                .entry(message.from)
                .and_modify(|at| *at = (*at).max(stamp))
                .or_insert(stamp);
        }
    }
    Ok(activity)
}
fn parse_time(raw: &str) -> Option<SystemTime> {
    let stamp =
        time::OffsetDateTime::parse(raw, &time::format_description::well_known::Rfc3339).ok()?;
    let nanos = u64::try_from(stamp.unix_timestamp_nanos()).ok()?;
    Some(SystemTime::UNIX_EPOCH + Duration::from_nanos(nanos))
}

pub fn expired_coordinator(
    route: &Route,
    live: bool,
    last: Option<SystemTime>,
    now: SystemTime,
) -> bool {
    route.kind == "coordinator" && !live && last.is_some_and(|at| old(now, at, DEAD_AGE))
}

/// A pane-less native route is protected unless the CLI supplies session-owner evidence.
pub fn protected(
    mail: &Path,
    routes: &BTreeMap<String, Route>,
    keep: Option<&str>,
) -> Result<BTreeSet<String>> {
    let mut live = BTreeSet::new();
    let registry = boop_harness::registry::Registry::discover();
    let store = bus::open_store(mail)?;
    let mut statement = store.connection().prepare("SELECT session.value, live.pid FROM agent_live live JOIN dict_session session ON session.id=live.session_id WHERE live.pid IS NOT NULL")?;
    for row in statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })? {
        let (session, pid) = row?;
        if u32::try_from(pid)
            .ok()
            .is_some_and(boop_harness::live::pid_alive)
        {
            live.insert(session.clone());
            for (name, route) in routes {
                if route.session_id.as_deref() == Some(session.as_str()) {
                    live.insert(name.clone());
                }
            }
        }
    }
    let trails = boop_store::trail::lanes_root()?;
    let owned = owned_routes(&trails, routes)?;
    for (name, route) in &owned {
        if Some(name.as_str()) == keep
            || route.kind == "native"
            || (route.kind == "coordinator"
                && live_session_owner(&registry, mail, name, route)?.is_some())
            || route
                .tmux
                .as_deref()
                .and_then(|pane| boop_store::tmux::mux().pane_pid(route.socket.as_deref(), pane))
                .is_some_and(boop_harness::live::pid_alive)
            || route.tmux.as_deref().is_some_and(|pane| {
                boop_store::tmux::mux().target_alive(route.socket.as_deref(), pane)
            })
        {
            live.insert(name.clone());
        }
    }
    // Unregistered sessions can still own targets and trails.
    let sessions = boop_store::tmux::mux().live_sessions(None);
    if let Some(sessions) = sessions {
        live.extend(
            sessions
                .names
                .into_iter()
                .filter(|name| boop_store::tmux::mux().target_alive(None, name)),
        );
    }

    Ok(live)
}

pub fn collect(
    root: &Path,
    trails: &Path,
    routes: &BTreeMap<String, Route>,
    live: &BTreeSet<String>,
    activity: &BTreeMap<String, SystemTime>,
    now: SystemTime,
) -> Result<Vec<Candidate>> {
    let mut out = Vec::new();
    for (base, kind, age, reason) in [
        (root, Kind::Target, TARGET_AGE, "target untouched 24h"),
        (trails, Kind::Trail, DEAD_AGE, "lane dead 7d"),
    ] {
        if !base.exists() {
            continue;
        }
        for entry in fs::read_dir(base)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == "_shared" || live.contains(&name) {
                continue;
            }
            let path = if kind == Kind::Target {
                entry.path().join("target")
            } else {
                entry.path()
            };
            if !path.is_dir() || !under(base, &path) {
                continue;
            }
            let Ok((bytes, touched)) = tree(&path) else {
                continue;
            };
            let touched = activity
                .get(&name)
                .copied()
                .unwrap_or(SystemTime::UNIX_EPOCH)
                .max(touched);
            if !old(now, touched, age) {
                continue;
            }
            let state = if routes.contains_key(&name) {
                "dead"
            } else if trails.join(&name).join("spawn.json").exists() {
                "retired"
            } else {
                "unregistered"
            };
            out.push(Candidate {
                path,
                bytes,
                lane: name,
                state,
                reason,
                kind: kind.clone(),
            });
        }
    }
    // Worktrees are authorized by route/spawn ownership, never by scanning arbitrary repositories.
    let owned = owned_routes(trails, routes)?;
    for (name, route) in &owned {
        if live.contains(name) {
            continue;
        }
        let Some(path) = route
            .worktree_dir
            .as_deref()
            .and_then(|p| boop_harness::worktree::deletable_worktree(Path::new(p)))
        else {
            continue;
        };
        let Some(repo) = boop_harness::worktree::worktree_owner(&path) else {
            continue;
        };
        let Some(branch) = worktree_branch(&path) else {
            continue;
        };
        let base = merged_base_branch(&repo, route.base_sha.as_deref(), None);
        if !branch_merged(&repo, &branch, &base) {
            continue;
        }
        if !git(&path, &["status", "--porcelain"]).is_some_and(|dirt| dirt.is_empty()) {
            continue;
        }
        let Ok((bytes, _)) = tree(&path) else {
            continue;
        };
        out.push(Candidate {
            path,
            bytes,
            lane: name.clone(),
            state: if routes.contains_key(name) {
                "dead"
            } else {
                "retired"
            },
            reason: "merged worktree",
            kind: Kind::Worktree { repo, branch, base },
        });
    }
    let live_worktrees: Vec<_> = owned
        .iter()
        .filter(|(name, _)| live.contains(*name))
        .filter_map(|(_, route)| {
            route
                .worktree_dir
                .as_deref()
                .and_then(|p| fs::canonicalize(p).ok())
        })
        .collect();
    out.retain(|candidate| {
        fs::canonicalize(&candidate.path).is_ok_and(|path| {
            !live_worktrees
                .iter()
                .any(|live| path.starts_with(live) || live.starts_with(&path))
        })
    });
    let priority = |kind: &Kind| match kind {
        Kind::Worktree { .. } => 0,
        Kind::Target => 1,
        Kind::Trail => 2,
    };
    out.sort_by(|a, b| {
        priority(&a.kind)
            .cmp(&priority(&b.kind))
            .then(a.path.cmp(&b.path))
    });
    out.dedup_by(|a, b| a.path == b.path && a.kind == b.kind);
    Ok(out)
}
fn git(repo: &Path, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
/// The branch a lane delete checks merge against: `--merged-into`, else the
/// branch whose tip is the lane's base sha, else `main`.
pub fn merged_base_branch(
    repo: &Path,
    base_sha: Option<&str>,
    merged_into: Option<&str>,
) -> String {
    if let Some(branch) = merged_into {
        return branch.to_owned();
    }
    if let Some(sha) = base_sha {
        let branches = git_lines(
            repo,
            &["branch", "--format=%(refname:short)", "--points-at", sha],
        );
        if let Some(main) = branches.iter().find(|branch| branch.as_str() == "main") {
            return main.clone();
        }
        if let Some(first) = branches.first() {
            return first.clone();
        }
    }
    "main".to_owned()
}

/// `git branch --merged <base>` lists `branch`.
pub fn branch_merged(repo: &Path, branch: &str, base: &str) -> bool {
    git_lines(
        repo,
        &["branch", "--merged", base, "--format=%(refname:short)"],
    )
    .iter()
    .any(|listed| listed == branch)
}

/// The branch checked out in a worktree, for a delete that only has the path.
pub fn worktree_branch(worktree: &Path) -> Option<String> {
    git_lines(worktree, &["symbolic-ref", "--short", "HEAD"])
        .into_iter()
        .next()
}

/// Remove one worktree and its branch when the branch is merged into the base;
/// otherwise keep the worktree and say so. Returns one line per outcome.
pub fn reclaim_merged_worktree(
    repo: &Path,
    worktree: &Path,
    branch: &str,
    base_sha: Option<&str>,
    merged_into: Option<&str>,
) -> Vec<String> {
    let base = merged_base_branch(repo, base_sha, merged_into);
    if !branch_merged(repo, branch, &base) {
        return vec![format!("kept worktree {} (unmerged)", worktree.display())];
    }
    match boop_harness::worktree::reclaim_carcass(repo, branch, worktree) {
        Ok(removed) => removed.lines(),
        Err(error) => vec![format!("kept worktree {} ({error})", worktree.display())],
    }
}

fn git_lines(repo: &Path, args: &[&str]) -> Vec<String> {
    git(repo, args)
        .map(|rows| rows.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

/// Call only with a candidate from a fresh collection, after checking lane liveness again.
pub fn remove(candidate: &Candidate, root: &Path, trails: &Path) -> Result<()> {
    match &candidate.kind {
        Kind::Target | Kind::Trail => {
            let base = if candidate.kind == Kind::Target {
                root
            } else {
                trails
            };
            anyhow::ensure!(under(base, &candidate.path), "gc path escaped owned root");
            let (_, touched) = tree(&candidate.path)?;
            let age = if candidate.kind == Kind::Target {
                TARGET_AGE
            } else {
                DEAD_AGE
            };
            anyhow::ensure!(old(SystemTime::now(), touched, age), "gc path was touched");
            fs::remove_dir_all(&candidate.path)?;
        }
        Kind::Worktree { repo, branch, base } => {
            anyhow::ensure!(
                boop_harness::worktree::deletable_worktree(&candidate.path).is_some(),
                "gc worktree ownership changed"
            );
            anyhow::ensure!(
                boop_harness::worktree::worktree_owner(&candidate.path).as_ref() == Some(repo),
                "gc worktree repository changed"
            );
            anyhow::ensure!(
                git(&candidate.path, &["symbolic-ref", "--short", "HEAD"]).as_deref()
                    == Some(branch),
                "gc worktree branch changed"
            );
            anyhow::ensure!(
                branch_merged(repo, branch, base),
                "gc worktree is no longer merged"
            );
            boop_harness::worktree::reclaim_carcass(repo, branch, &candidate.path)?;
        }
    }
    Ok(())
}

pub fn live_session_owner(
    registry: &boop_harness::registry::Registry,
    dir: &Path,
    name: &str,
    route: &boop_store::bus::Route,
) -> Result<Option<String>> {
    let db = boop_store::bus::db_path(dir)?;
    if boop_store::bus::try_route_lock(&db, name, "native-tui")?.is_none() {
        return Ok(Some("native TUI wrapper holds the route lock".into()));
    }
    // A conversation may have been resumed under another route name.
    for (other_name, other) in boop_store::bus::read_routes(dir)? {
        if other_name != name
            && route.session_id.is_some()
            && other.harness == route.harness
            && other.session_id == route.session_id
            && boop_store::bus::try_route_lock(&db, &other_name, "native-tui")?.is_none()
        {
            return Ok(Some(format!("native TUI wrapper holds route {other_name}")));
        }
    }
    let (Some(harness), Some(session)) = (route.harness, route.session_id.as_deref()) else {
        return Ok(None);
    };
    // Native registries that supply a process can also own a conversation
    // outside boop. Codex's historical thread rows have no pid and do not
    // establish ownership merely by existing.
    Ok(registry
        .get(harness)
        .live()
        .live_sessions()?
        .into_iter()
        .find(|live| {
            live.session_id == session && live.pid.is_some_and(boop_harness::live::pid_alive)
        })
        .and_then(|live| live.pid)
        .map(|pid| format!("harness session runs as process {pid}")))
}

/// The disk floor uses the same candidates and removal checks as the command.
pub fn reclaim_until_floor(
    mail: &Path,
    root: &Path,
    floor: f64,
    keep: Option<&str>,
) -> Option<f64> {
    let trails = boop_store::trail::lanes_root().ok()?;
    let mut free = crate::supervise::free_disk_gb(root)?;
    while free < floor {
        let routes = bus::read_routes(mail).ok()?;
        let live = protected(mail, &routes, keep).ok()?;
        let activity = activity(mail, &routes).ok()?;
        let candidates = exclude_store(
            mail,
            collect(root, &trails, &routes, &live, &activity, SystemTime::now()).ok()?,
        )
        .ok()?;
        let Some(candidate) = candidates.first() else {
            break;
        };
        if let Err(error) = remove(candidate, root, &trails) {
            tracing::warn!(path = %candidate.path.display(), %error, "disk floor gc failed");
            break;
        }
        tracing::info!(path = %candidate.path.display(), lane = candidate.lane, "disk floor gc reclaimed");
        free = crate::supervise::free_disk_gb(root)?;
    }
    Some(free)
}

/// Exclude a resource containing the addressed store or mailbox, including custom store names.
pub fn exclude_store(mail: &Path, candidates: Vec<Candidate>) -> Result<Vec<Candidate>> {
    let db = bus::db_path(mail)?;
    let mail = fs::canonicalize(mail)?;
    let db = fs::canonicalize(db)?;
    Ok(candidates
        .into_iter()
        .filter(|candidate| {
            fs::canonicalize(&candidate.path)
                .is_ok_and(|path| !mail.starts_with(&path) && !db.starts_with(&path))
        })
        .collect())
}

#[cfg(test)]
mod _3_gc_tests;

fn owned_routes(
    trails: &Path,
    routes: &BTreeMap<String, Route>,
) -> Result<BTreeMap<String, Route>> {
    let mut owned = routes.clone();
    if !trails.exists() {
        return Ok(owned);
    }
    for entry in fs::read_dir(trails)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let spawn_path = entry.path().join("spawn.json");
        if !fs::symlink_metadata(&spawn_path).is_ok_and(|meta| meta.file_type().is_file()) {
            continue;
        }
        let Ok(raw) = fs::read(spawn_path) else {
            continue;
        };
        let Ok(spawn) = serde_json::from_slice::<boop_store::trail::Spawn>(&raw) else {
            continue;
        };
        let route = owned
            .entry(name)
            .or_insert_with(|| bus::route_from_value(&spawn.route));
        route.socket = spawn.socket;
        if route.tmux.is_none() && route.kind == "lane" {
            route.tmux = Some(spawn.tmux);
        }
    }
    Ok(owned)
}

pub fn coordinator_live(
    registry: &boop_harness::registry::Registry,
    mail: &Path,
    name: &str,
    route: &Route,
) -> Result<bool> {
    Ok(live_session_owner(registry, mail, name, route)?.is_some()
        || route.tmux.as_deref().is_some_and(|pane| {
            boop_store::tmux::mux().target_alive(route.socket.as_deref(), pane)
        })
        || route
            .tmux
            .as_deref()
            .and_then(|pane| boop_store::tmux::mux().pane_pid(route.socket.as_deref(), pane))
            .is_some_and(boop_harness::live::pid_alive))
}
