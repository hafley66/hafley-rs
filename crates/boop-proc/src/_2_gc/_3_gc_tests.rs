use super::*;
use std::{
    fs::File,
    sync::atomic::{AtomicU64, Ordering},
};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "boop-gc-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn dir(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(&path).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn age(path: &Path, at: SystemTime) {
    if path.is_dir() {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if !entry.file_type().unwrap().is_symlink() {
                age(&entry.path(), at);
            }
        }
    }
    File::open(path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(at))
        .unwrap();
}

#[test]
fn collection_and_application_preserve_live_fresh_shared_outside_mail_and_store() {
    let f = Fixture::new();
    let root = f.dir("targets");
    let trails = f.dir("trails");
    let now = SystemTime::now();
    let old = now - DEAD_AGE - Duration::from_secs(60);
    for name in [
        "dead",
        "stray",
        "live",
        "fresh",
        "nested-fresh",
        "_shared",
        "store",
        "mail-owner",
    ] {
        let target = f.dir(&format!("targets/{name}/target"));
        fs::write(target.join("object"), name).unwrap();
        age(&target, old);
    }
    age(&root.join("fresh/target"), now);
    fs::write(root.join("nested-fresh/target/object"), "new write").unwrap();
    File::open(root.join("nested-fresh/target"))
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(old))
        .unwrap();
    for suffix in ["", "-wal", "-shm"] {
        fs::write(
            root.join("store/target").join(format!("boop.db{suffix}")),
            suffix,
        )
        .unwrap();
    }
    f.dir("targets/mail-owner/target/mail");
    age(&root.join("store/target"), old);
    age(&root.join("mail-owner/target"), old);
    let outside = f.dir("outside/target");
    age(&outside, old);
    std::os::unix::fs::symlink(f.0.join("outside"), root.join("escape")).unwrap();
    f.dir("targets/escape-target");
    std::os::unix::fs::symlink(&outside, root.join("escape-target/target")).unwrap();
    for name in ["dead", "live", "recent"] {
        let trail = f.dir(&format!("trails/{name}"));
        fs::write(trail.join("supervise.log"), name).unwrap();
        age(&trail, old);
    }
    let routes = BTreeMap::from([
        ("dead".into(), Route::default()),
        ("live".into(), Route::default()),
    ]);
    let live = BTreeSet::from(["live".into()]);
    let activity = BTreeMap::from([("recent".into(), now)]);
    let candidates = collect(&root, &trails, &routes, &live, &activity, now).unwrap();
    let rows: Vec<_> = candidates
        .iter()
        .map(|c| {
            (
                c.path
                    .strip_prefix(&f.0)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                c.bytes,
                c.lane.as_str(),
                c.state,
                c.reason,
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            (
                "targets/dead/target".into(),
                4,
                "dead",
                "dead",
                "target untouched 24h"
            ),
            (
                "targets/stray/target".into(),
                5,
                "stray",
                "unregistered",
                "target untouched 24h"
            ),
            ("trails/dead".into(), 4, "dead", "dead", "lane dead 7d"),
        ]
    );
    for candidate in &candidates {
        remove(candidate, &root, &trails).unwrap();
    }
    for candidate in &candidates {
        assert!(!candidate.path.exists());
    }
    for path in [
        "targets/live/target",
        "targets/fresh/target",
        "targets/nested-fresh/target",
        "targets/_shared/target",
        "targets/store/target/boop.db",
        "targets/store/target/boop.db-wal",
        "targets/store/target/boop.db-shm",
        "targets/mail-owner/target/mail",
        "outside/target",
        "trails/live",
        "trails/recent",
    ] {
        assert!(f.0.join(path).exists(), "preserve {path}");
    }
    let forged = Candidate {
        path: outside.clone(),
        bytes: 0,
        lane: "forged".into(),
        state: "dead",
        reason: "forged",
        kind: Kind::Target,
    };
    assert!(remove(&forged, &root, &trails).is_err());
    assert!(outside.exists());
    assert!(!under(&root, &root));
    assert!(!under(&root, &root.join("escape/new-target")));
    std::os::unix::fs::symlink(f.0.join("missing-outside"), root.join("dangling")).unwrap();
    assert!(!under(&root, &root.join("dangling/new-target")));
    std::os::unix::fs::symlink("cycle-b", root.join("cycle-a")).unwrap();
    std::os::unix::fs::symlink("cycle-a", root.join("cycle-b")).unwrap();
    assert!(!under(&root, &root.join("cycle-a/new-target")));
    assert!(!under(&root, &root.join("../outside/target")));
    assert!(under(&root, &root.join("new/target")));
}

#[test]
fn thresholds_use_latest_lane_activity_and_recheck_new_writes() {
    let f = Fixture::new();
    let root = f.dir("targets");
    let trails = f.dir("trails");
    let target = f.dir("targets/dead/target");
    let now = SystemTime::now();
    age(&target, now - TARGET_AGE);
    let routes = BTreeMap::new();
    let live = BTreeSet::new();
    assert!(collect(
        &root,
        &trails,
        &routes,
        &live,
        &BTreeMap::new(),
        now - Duration::from_secs(1)
    )
    .unwrap()
    .is_empty());
    let candidates = collect(&root, &trails, &routes, &live, &BTreeMap::new(), now).unwrap();
    assert_eq!(candidates.len(), 1);
    assert!(collect(
        &root,
        &trails,
        &routes,
        &live,
        &BTreeMap::from([("dead".into(), now)]),
        now
    )
    .unwrap()
    .is_empty());
    fs::write(target.join("new"), "live write").unwrap();
    assert!(remove(&candidates[0], &root, &trails).is_err());
    let coordinator = Route {
        kind: "coordinator".into(),
        ..Route::default()
    };
    let lane = Route::default();
    assert_eq!(
        [
            expired_coordinator(&coordinator, false, Some(now - DEAD_AGE), now),
            expired_coordinator(
                &coordinator,
                false,
                Some(now - DEAD_AGE + Duration::from_secs(1)),
                now
            ),
            expired_coordinator(&coordinator, true, Some(now - DEAD_AGE), now),
            expired_coordinator(&coordinator, false, None, now),
            expired_coordinator(&lane, false, Some(now - DEAD_AGE), now),
        ],
        [true, false, false, false, false]
    );
}

#[test]
fn merged_worktrees_require_ownership_cleanliness_and_a_dead_lane() {
    let f = Fixture::new();
    let root = f.dir("targets");
    let trails = f.dir("trails");
    let repo = f.dir("repo");
    let command = |cwd: &Path, args: &[&str]| {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    command(&repo, &["init", "-q", "-b", "main"]);
    command(&repo, &["config", "user.email", "test@example.invalid"]);
    command(&repo, &["config", "user.name", "test"]);
    fs::write(repo.join("seed"), "seed").unwrap();
    command(&repo, &["add", "seed"]);
    command(&repo, &["commit", "-qm", "seed"]);
    let mut routes = BTreeMap::new();
    for name in ["merged", "unmerged", "dirty", "live", "unowned"] {
        let path = f.0.join(name);
        command(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                path.to_str().unwrap(),
                "-b",
                name,
                "main",
            ],
        );
        if name != "unowned" {
            routes.insert(
                name.into(),
                Route {
                    worktree_dir: Some(path.to_string_lossy().into_owned()),
                    ..Route::default()
                },
            );
        }
    }
    fs::write(f.0.join("unmerged/change"), "unique").unwrap();
    command(&f.0.join("unmerged"), &["add", "change"]);
    command(&f.0.join("unmerged"), &["commit", "-qm", "unmerged"]);
    fs::write(f.0.join("dirty/change"), "uncommitted").unwrap();
    routes.insert(
        "main-tree".into(),
        Route {
            worktree_dir: Some(repo.to_string_lossy().into_owned()),
            ..Route::default()
        },
    );
    routes.insert(
        "dead-alias".into(),
        Route {
            worktree_dir: Some(f.0.join("live").to_string_lossy().into_owned()),
            ..Route::default()
        },
    );
    let candidates = collect(
        &root,
        &trails,
        &routes,
        &BTreeSet::from(["live".into()]),
        &BTreeMap::new(),
        SystemTime::now(),
    )
    .unwrap();
    assert_eq!(
        candidates
            .iter()
            .map(|c| (c.lane.as_str(), c.reason))
            .collect::<Vec<_>>(),
        [("merged", "merged worktree")]
    );
    remove(&candidates[0], &root, &trails).unwrap();
    assert!(!f.0.join("merged").exists());
    assert!(git(&repo, &["rev-parse", "--verify", "merged"]).is_none());
    for name in ["unmerged", "dirty", "live", "unowned", "repo"] {
        assert!(f.0.join(name).exists());
    }
}

#[test]
fn a_retired_worktree_is_collected_before_its_ownership_trail() {
    let f = Fixture::new();
    let root = f.dir("targets");
    let trails = f.dir("trails");
    let repo = boop_store::testing::TempRepo::new();
    let seed_branch = git(&repo.dir, &["symbolic-ref", "--short", "HEAD"]).unwrap();
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo.dir)
        .args(["worktree", "add", "-q", "-b", "retired"])
        .arg(&repo.worktree)
        .output()
        .unwrap();
    assert!(output.status.success());
    let route = Route {
        worktree_dir: Some(repo.worktree.to_string_lossy().into_owned()),
        base_sha: Some(repo.sha.clone()),
        ..Route::default()
    };
    let trail = f.dir("trails/retired");
    let spawn = boop_store::trail::Spawn {
        tmux: "retired".into(),
        socket: None,
        cwd: repo.worktree.to_string_lossy().into_owned(),
        command: "false".into(),
        route: bus::route_to_value(&route),
        spawn_id: None,
        post_pr: false,
        pr_base: None,
    };
    fs::write(
        trail.join("spawn.json"),
        serde_json::to_vec(&spawn).unwrap(),
    )
    .unwrap();
    age(
        &trail,
        SystemTime::now() - DEAD_AGE - Duration::from_secs(60),
    );
    let candidates = collect(
        &root,
        &trails,
        &BTreeMap::new(),
        &BTreeSet::new(),
        &BTreeMap::new(),
        SystemTime::now(),
    )
    .unwrap();
    assert_eq!(
        candidates.iter().map(|c| c.reason).collect::<Vec<_>>(),
        ["merged worktree", "lane dead 7d"]
    );
    if let Kind::Worktree { base, .. } = &candidates[0].kind {
        assert_eq!(base, &seed_branch);
    }
    for candidate in candidates {
        remove(&candidate, &root, &trails).unwrap();
    }
    assert!(!repo.worktree.exists());
    assert!(!trail.exists());
}

#[test]
fn advertised_targets_retain_the_owner_and_protect_live_overrides() {
    let f = Fixture::new();
    let root = f.dir("targets");
    let trails = f.dir("trails");
    let now = SystemTime::now();
    let old = now - TARGET_AGE - Duration::from_secs(60);
    for (lane, relative) in [
        ("live-owner", "unregistered-name/target"),
        ("dead-owner", "custom/build-output"),
    ] {
        let target = f.dir(&format!("targets/{relative}"));
        fs::write(target.join("object"), lane).unwrap();
        age(&target, old);
        let trail = f.dir(&format!("trails/{lane}"));
        fs::write(
            trail.join("target.json"),
            serde_json::to_vec(&target).unwrap(),
        )
        .unwrap();
    }
    let routes = BTreeMap::from([
        ("live-owner".into(), Route::default()),
        ("dead-owner".into(), Route::default()),
    ]);
    let candidates = collect(
        &root,
        &trails,
        &routes,
        &BTreeSet::from(["live-owner".into()]),
        &BTreeMap::new(),
        now,
    )
    .unwrap();
    assert_eq!(
        candidates
            .iter()
            .map(|c| (
                c.lane.as_str(),
                c.path.strip_prefix(&root).unwrap().to_str().unwrap()
            ))
            .collect::<Vec<_>>(),
        [("dead-owner", "custom/build-output")]
    );
    remove(&candidates[0], &root, &trails).unwrap();
    assert!(root.join("unregistered-name/target/object").exists());
    assert!(!root.join("custom/build-output").exists());
}
