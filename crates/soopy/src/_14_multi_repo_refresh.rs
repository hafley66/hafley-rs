//! Deterministic local many-repository refresh/load harness.
//!
//! The harness uses independent checkouts and bare remotes, then runs the
//! policy-gated fetch, ref, worktree, and watcher surfaces through a bounded
//! worker pool. It is deliberately local-only: no network URL is involved.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{ensure, Context, Result};
use serde::Serialize;
use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::{
    Acquisition, AcquisitionOperation, AcquisitionPolicy, AcquisitionRequest, Pattern, RefQuery,
    Revision, SourceQuery, SourceTree, WatchCoalescing, WatchQuery,
};

const DEFAULT_REPOSITORIES: usize = 32;
const DEFAULT_ROUNDS: usize = 3;
const DEFAULT_CONCURRENCY: usize = 4;
const RSS_SAMPLE_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Clone, Debug)]
pub struct MultiRepoRefreshConfig {
    pub repositories: usize,
    pub rounds: usize,
    pub concurrency: usize,
    pub root: Option<PathBuf>,
    pub keep: bool,
}

impl Default for MultiRepoRefreshConfig {
    fn default() -> Self {
        Self {
            repositories: DEFAULT_REPOSITORIES,
            rounds: DEFAULT_ROUNDS,
            concurrency: DEFAULT_CONCURRENCY,
            root: None,
            keep: false,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct RssSample {
    pub elapsed_ms: u128,
    pub round: usize,
    pub rss_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct MultiRepoRefreshRoundReceipt {
    pub round: usize,
    pub repositories: usize,
    pub fetched_operations: usize,
    pub watcher_delta_batches: usize,
    pub worktree_observations: usize,
    pub elapsed_ms: u128,
    pub child_processes: usize,
    pub retained_cache_bytes: u64,
    pub rss_peak_bytes: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MultiRepoRefreshReceipt {
    pub requested_repositories: usize,
    pub effective_repositories: usize,
    pub requested_rounds: usize,
    pub effective_rounds: usize,
    pub concurrency: usize,
    pub fixture_bytes: u64,
    pub phase_times_ms: BTreeMap<String, u128>,
    pub child_processes: usize,
    pub retained_cache_bytes: u64,
    pub retained_cache_growth_limit_bytes: u64,
    pub rss_peak_bytes: Option<u64>,
    pub rss_growth_limit_bytes: Option<u64>,
    pub rss_samples: Vec<RssSample>,
    pub rounds: Vec<MultiRepoRefreshRoundReceipt>,
}

#[derive(Clone)]
struct RepoFixture {
    remote: PathBuf,
    checkout: PathBuf,
    index: usize,
}

#[derive(Default)]
struct RoundWork {
    fetched_operations: usize,
    watcher_delta_batches: usize,
    worktree_observations: usize,
}

/// Run the bounded local many-repository refresh scenario and return its
/// machine-readable receipt. The generated fixture is removed on success
/// unless `keep` is set or an explicit root was supplied.
pub fn run_multi_repo_refresh(config: &MultiRepoRefreshConfig) -> Result<MultiRepoRefreshReceipt> {
    ensure!(
        config.repositories > 0,
        "repositories must be greater than zero"
    );
    ensure!(config.rounds > 0, "rounds must be greater than zero");
    ensure!(
        config.concurrency > 0,
        "concurrency must be greater than zero"
    );

    let started = Instant::now();
    let (fixture_root, remove_after) = fixture_root(config)?;
    let setup_started = Instant::now();
    let fixtures = build_fixture(&fixture_root, config.repositories, config.rounds)?;
    let fixture_bytes = retained_cache_bytes(&fixtures)?;
    let setup_ms = setup_started.elapsed().as_millis();

    let instrumentation = Instrumentation::install(&fixture_root)?;
    let mut samples = Vec::new();
    let mut rounds = Vec::with_capacity(config.rounds);
    let refresh_started = Instant::now();
    for round in 0..config.rounds {
        let round_started = Instant::now();
        let child_before = instrumentation.count()?;
        let sample_store = Arc::new(Mutex::new(Vec::new()));
        let stop_sampling = Arc::new(AtomicBool::new(false));
        let sample_started = refresh_started;
        let sample_store_for_thread = Arc::clone(&sample_store);
        let stop_for_thread = Arc::clone(&stop_sampling);
        let worker_count = config.concurrency.min(fixtures.len());
        let workers = thread::scope(|scope| {
            let sampler = scope.spawn(move || {
                sample_rss_loop(
                    round,
                    sample_started,
                    stop_for_thread,
                    sample_store_for_thread,
                )
            });
            let mut handles = Vec::with_capacity(worker_count);
            let fixtures_ref = fixtures.as_slice();
            for worker in 0..worker_count {
                handles.push(scope.spawn(move || -> Result<Vec<RoundWork>> {
                    let mut work = Vec::new();
                    for fixture in fixtures_ref.iter().skip(worker).step_by(worker_count) {
                        work.push(refresh_one(fixture, round)?);
                    }
                    Ok(work)
                }));
            }
            let mut joined = Vec::new();
            for handle in handles {
                let result = handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("refresh worker panicked"))??;
                joined.extend(result);
            }
            stop_sampling.store(true, Ordering::Relaxed);
            sampler
                .join()
                .map_err(|_| anyhow::anyhow!("RSS sampler panicked"))?;
            Ok::<_, anyhow::Error>(joined)
        })?;

        let sample_round = sample_store
            .lock()
            .expect("RSS sample mutex poisoned")
            .clone();
        let round_rss_peak = sample_round.iter().map(|sample| sample.rss_bytes).max();
        samples.extend(sample_round);
        let mut aggregate = RoundWork::default();
        for work in workers {
            aggregate.fetched_operations += work.fetched_operations;
            aggregate.watcher_delta_batches += work.watcher_delta_batches;
            aggregate.worktree_observations += work.worktree_observations;
        }
        let retained = retained_cache_bytes(&fixtures)?;
        let child_after = instrumentation.count()?;
        rounds.push(MultiRepoRefreshRoundReceipt {
            round,
            repositories: fixtures.len(),
            fetched_operations: aggregate.fetched_operations,
            watcher_delta_batches: aggregate.watcher_delta_batches,
            worktree_observations: aggregate.worktree_observations,
            elapsed_ms: round_started.elapsed().as_millis(),
            child_processes: child_after.saturating_sub(child_before),
            retained_cache_bytes: retained,
            rss_peak_bytes: round_rss_peak,
        });
    }
    let refresh_ms = refresh_started.elapsed().as_millis();
    let child_processes = instrumentation.count()?;
    instrumentation.uninstall();

    let retained_cache_bytes = retained_cache_bytes(&fixtures)?;
    let retained_cache_growth_limit_bytes = fixture_bytes.saturating_mul(2).max(fixture_bytes + 1);
    ensure!(
        rounds
            .iter()
            .all(|round| round.retained_cache_bytes <= retained_cache_growth_limit_bytes),
        "retained cache grew beyond 2x fixture bytes"
    );

    let cold_rss = rounds.first().and_then(|round| round.rss_peak_bytes);
    let rss_growth_limit_bytes = cold_rss.map(|cold| cold / 4 + fixture_bytes.saturating_mul(4));
    if let Some(limit) = rss_growth_limit_bytes {
        for round in rounds.iter().skip(1) {
            if let Some(peak) = round.rss_peak_bytes {
                ensure!(
                    peak <= cold_rss.unwrap_or(peak).saturating_add(limit),
                    "warm RSS exceeded cold RSS plus the 25% plus fixture allowance"
                );
            }
        }
    }

    let mut phase_times_ms = BTreeMap::new();
    phase_times_ms.insert("fixture_setup".to_owned(), setup_ms);
    phase_times_ms.insert("refresh".to_owned(), refresh_ms);
    phase_times_ms.insert("total".to_owned(), started.elapsed().as_millis());
    let receipt = MultiRepoRefreshReceipt {
        requested_repositories: config.repositories,
        effective_repositories: fixtures.len(),
        requested_rounds: config.rounds,
        effective_rounds: rounds.len(),
        concurrency: config.concurrency.min(fixtures.len()),
        fixture_bytes,
        phase_times_ms,
        child_processes,
        retained_cache_bytes,
        retained_cache_growth_limit_bytes,
        rss_peak_bytes: samples.iter().map(|sample| sample.rss_bytes).max(),
        rss_growth_limit_bytes,
        rss_samples: samples,
        rounds,
    };
    if remove_after && !config.keep {
        fs::remove_dir_all(&fixture_root)
            .with_context(|| format!("remove generated fixture {}", fixture_root.display()))?;
    }
    Ok(receipt)
}

fn fixture_root(config: &MultiRepoRefreshConfig) -> Result<(PathBuf, bool)> {
    match &config.root {
        Some(root) => {
            ensure!(
                !root.exists(),
                "explicit harness root already exists: {}",
                root.display()
            );
            fs::create_dir_all(root).with_context(|| format!("create {}", root.display()))?;
            Ok((root.clone(), false))
        }
        None => {
            let suffix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .context("system clock before Unix epoch")?
                .as_nanos();
            let root = std::env::temp_dir()
                .join(format!("soopy-multi-repo-{}-{suffix}", std::process::id()));
            fs::create_dir_all(&root).with_context(|| format!("create {}", root.display()))?;
            Ok((root, true))
        }
    }
}

fn build_fixture(root: &Path, repositories: usize, rounds: usize) -> Result<Vec<RepoFixture>> {
    let seed = root.join("seed");
    fs::create_dir_all(seed.join("src"))?;
    git(&seed, &["init", "-q", "-b", "main"])?;
    fs::write(seed.join("src/lib.rs"), "pub const VALUE: u8 = 0;\n")?;
    git(&seed, &["add", "."])?;
    git(&seed, &["commit", "-qm", "fixture-0"])?;
    let mut commits = vec![git(&seed, &["rev-parse", "HEAD"])?];
    for round in 1..=rounds {
        fs::write(
            seed.join("src/lib.rs"),
            format!("pub const VALUE: u8 = {round};\n"),
        )?;
        git(&seed, &["add", "."])?;
        git(&seed, &["commit", "-qm", &format!("fixture-{round}")])?;
        commits.push(git(&seed, &["rev-parse", "HEAD"])?);
    }
    for round in 0..rounds {
        git(
            &seed,
            &["branch", &format!("refresh-{round}"), &commits[round + 1]],
        )?;
    }

    let mut fixtures = Vec::with_capacity(repositories);
    for index in 0..repositories {
        let remote = root.join(format!("remote-{index}.git"));
        let checkout = root.join(format!("checkout-{index}"));
        git(
            root,
            &[
                "clone",
                "-q",
                "--bare",
                seed.to_str().context("seed path is UTF-8")?,
                remote.to_str().context("remote path is UTF-8")?,
            ],
        )?;
        git(
            root,
            &[
                "clone",
                "-q",
                remote.to_str().context("remote path is UTF-8")?,
                checkout.to_str().context("checkout path is UTF-8")?,
            ],
        )?;
        git(&checkout, &["checkout", "-q", "-B", "main", &commits[0]])?;
        git(&checkout, &["update-ref", "-d", "refs/remotes/origin/main"])?;
        fixtures.push(RepoFixture {
            remote,
            checkout,
            index,
        });
    }
    Ok(fixtures)
}

fn refresh_one(fixture: &RepoFixture, round: usize) -> Result<RoundWork> {
    let repository = crate::open(&fixture.checkout)?;
    let tree = SourceTree::open(repository.clone());
    let query = WatchQuery {
        source: Some(SourceQuery {
            revision: Revision::Worktree,
            patterns: vec![Pattern("**/*.rs".to_owned())],
        }),
        refs: Some(RefQuery {
            repository: repository.identity.clone(),
            namespace: "refs/remotes/origin".into(),
            name: None,
            pattern: None,
        }),
        index: true,
        linked_worktrees: true,
        coalescing: WatchCoalescing {
            quiet_ms: 20,
            max_ms: 200,
        },
    };
    let mut watcher = tree.watch_repository(query)?;
    let worktree_observations = watcher
        .snapshot()
        .worktrees
        .as_ref()
        .map_or(0, |snapshot| snapshot.worktrees.len());
    let acquisition = Acquisition::open(repository.clone());
    let operation = AcquisitionOperation::FetchRef {
        remote: "origin".into(),
        name: format!("refresh-{round}").into(),
    };
    let outcomes = acquisition.execute(
        &AcquisitionPolicy {
            allow_fetch: true,
            allow_tag_fetch: false,
            allow_unshallow: false,
        },
        &AcquisitionRequest {
            repository: repository.identity,
            operations: vec![operation],
        },
    )?;
    ensure!(
        outcomes.len() == 1,
        "one fetch outcome per refresh operation"
    );
    fs::write(
        fixture
            .checkout
            .join("src")
            .join(format!("refresh-{round}.rs")),
        format!("pub const REPOSITORY: usize = {};\n", fixture.index),
    )?;
    let deltas = watcher
        .recv_timeout(Duration::from_secs(5))?
        .context("watcher did not observe fetch or worktree refresh")?;
    ensure!(
        !deltas.is_empty(),
        "watcher returned no logical refresh deltas"
    );
    Ok(RoundWork {
        fetched_operations: outcomes.len(),
        watcher_delta_batches: 1,
        worktree_observations,
    })
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_AUTHOR_NAME", "soopy-refresh")
        .env("GIT_AUTHOR_EMAIL", "soopy-refresh@example.invalid")
        .env("GIT_COMMITTER_NAME", "soopy-refresh")
        .env("GIT_COMMITTER_EMAIL", "soopy-refresh@example.invalid")
        .output()
        .with_context(|| format!("run git {args:?}"))?;
    ensure!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn retained_cache_bytes(fixtures: &[RepoFixture]) -> Result<u64> {
    fixtures.iter().try_fold(0_u64, |total, fixture| {
        let remote = directory_bytes(&fixture.remote)?;
        let checkout_git = directory_bytes(&fixture.checkout.join(".git"))?;
        total
            .checked_add(remote)
            .and_then(|sum| sum.checked_add(checkout_git))
            .context("retained cache byte count overflow")
    })
}

fn directory_bytes(path: &Path) -> Result<u64> {
    if !path.exists() {
        return Ok(0);
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Ok(0);
    }
    fs::read_dir(path)?.try_fold(0_u64, |total, entry| {
        let entry = entry?;
        total
            .checked_add(directory_bytes(&entry.path())?)
            .context("directory byte count overflow")
    })
}

fn sample_rss_loop(
    round: usize,
    started: Instant,
    stop: Arc<AtomicBool>,
    samples: Arc<Mutex<Vec<RssSample>>>,
) {
    let mut system = System::new();
    loop {
        if let Some(rss_bytes) = current_rss(&mut system) {
            samples
                .lock()
                .expect("RSS sample mutex poisoned")
                .push(RssSample {
                    elapsed_ms: started.elapsed().as_millis(),
                    round,
                    rss_bytes,
                });
        }
        if stop.load(Ordering::Relaxed) {
            break;
        }
        thread::sleep(RSS_SAMPLE_INTERVAL);
    }
}

fn current_rss(system: &mut System) -> Option<u64> {
    let pid = Pid::from_u32(std::process::id());
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).map(|process| process.memory())
}

struct Instrumentation {
    old_path: Option<std::ffi::OsString>,
    count_path: PathBuf,
}

impl Instrumentation {
    fn install(root: &Path) -> Result<Self> {
        let real_git = Command::new("sh")
            .args(["-c", "command -v git"])
            .output()
            .context("find real Git executable")?;
        ensure!(real_git.status.success(), "could not locate git executable");
        let real_git = String::from_utf8(real_git.stdout)?.trim().to_owned();
        let bin = root.join("instrument-bin");
        fs::create_dir_all(&bin)?;
        let wrapper = bin.join("git");
        fs::write(
            &wrapper,
            "#!/bin/sh\nprintf '%s\\n' \"$1\" >> \"$SOOPY_GIT_COUNT\"\nexec \"$SOOPY_REAL_GIT\" \"$@\"\n",
        )?;
        let mut permissions = fs::metadata(&wrapper)?.permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(0o755);
            fs::set_permissions(&wrapper, permissions)?;
        }
        let count_path = root.join("git-child-count.log");
        fs::write(&count_path, "")?;
        let old_path = std::env::var_os("PATH");
        let mut path = bin.into_os_string();
        if let Some(old) = &old_path {
            path.push(":");
            path.push(old);
        }
        std::env::set_var("PATH", path);
        std::env::set_var("SOOPY_REAL_GIT", real_git);
        std::env::set_var("SOOPY_GIT_COUNT", &count_path);
        Ok(Self {
            old_path,
            count_path,
        })
    }

    fn count(&self) -> Result<usize> {
        Ok(fs::read_to_string(&self.count_path)?.lines().count())
    }

    fn uninstall(self) {
        if let Some(path) = self.old_path {
            std::env::set_var("PATH", path);
        }
        std::env::remove_var("SOOPY_REAL_GIT");
        std::env::remove_var("SOOPY_GIT_COUNT");
    }
}
