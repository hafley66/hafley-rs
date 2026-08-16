use soopy::{run_multi_repo_refresh, MultiRepoRefreshConfig};

#[test]
fn golden_many_repository_refresh_receipt() {
    let receipt = run_multi_repo_refresh(&MultiRepoRefreshConfig {
        repositories: 8,
        rounds: 2,
        concurrency: 2,
        root: None,
        keep: false,
    })
    .unwrap();

    assert_eq!(receipt.requested_repositories, 8);
    assert_eq!(receipt.effective_repositories, 8);
    assert_eq!(receipt.requested_rounds, 2);
    assert_eq!(receipt.effective_rounds, 2);
    assert_eq!(receipt.concurrency, 2);
    assert_eq!(
        receipt
            .rounds
            .iter()
            .map(|round| round.repositories)
            .sum::<usize>(),
        16
    );
    assert_eq!(
        receipt
            .rounds
            .iter()
            .map(|round| round.fetched_operations)
            .sum::<usize>(),
        16
    );
    assert_eq!(
        receipt
            .rounds
            .iter()
            .map(|round| round.watcher_delta_batches)
            .sum::<usize>(),
        16
    );
    assert_eq!(
        receipt
            .rounds
            .iter()
            .map(|round| round.worktree_observations)
            .sum::<usize>(),
        16
    );
    assert!(receipt.child_processes >= 16);
    assert!(receipt.rss_peak_bytes.is_some());
    assert!(!receipt.rss_samples.is_empty());
    assert!(receipt.retained_cache_bytes <= receipt.retained_cache_growth_limit_bytes);
    assert!(receipt
        .rounds
        .iter()
        .all(|round| round.rss_peak_bytes.is_some()));
}
