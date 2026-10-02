# Make boop's real-TUI + llmock e2e tier green on main (branch fix/e2e-llmock-green, base main)

Tier: crates/boop/tests/ e2e files that run real claude/codex/opencode/omp TUIs against loopback llmock
(~/.cargo/bin/llmock v0.1.2) inside a throwaway tmux server with a scratch store. Recipe: boop-harness
src/harness/mock_tui.rs; `boop --help` section near "llmock".

## Failing on main 2d666ded (coordinator run, sandboxed HOME, `cargo test --no-fail-fast -p boop`)
- tui_revive_e2e::a_dead_coordinator_pane_revives_on_its_session_{claude,codex,opencode}
  tui_revive_e2e.rs:510 "claude: row is not a revivable dead row: live  revive-e2e-claude coordinator claude  0h"
- worktree_reclaim_e2e::worktree_reclaim_{claude,codex,opencode}
  worktree_reclaim_e2e.rs:689 "claude case 3: both retired targets evicted"
- omp_live_trait_e2e: omp_live_panes_bind_distinct_sessions_and_project_real_transcripts (:508 "real OMP transcript by UUID"),
  tmux_command_preserves_homes_and_serializes_fixture_overrides (:206)
- t1_harness_boundaries::behavioral_harness_dispatch_stays_in_adapters (1_harness_boundaries.rs:331) flags
  crates/boop-store/src/1_user_slice.rs:26 FavoriteSourceKind::as_str as dispatch:match. Decide: that match is over a
  closed source-kind enum, not harness dispatch; fix the lint's classification or the code, whichever is wrong, and say why.

## Work
1. Run each file alone, `--test-threads=1`, HOME=<worktree>/scratch/home, BOOP_DB/BOOP_MAIL_DIR unset. Record first failure.
2. Root-cause each: product bug (fix product) vs test/env assumption (fix test so it owns its scratch resources).
   Prove which by evidence: the boop db rows, tmux capture, llmock request log, transcript path.
   Hermetic-merge suspects: 09896805 changed test path roots (crates/boop-store/src/_0_test_paths.rs thread-local root,
   supervisor thread inheritance, subprocess env stamping in boop-harness harness/_0_test_env.rs). A child process or
   thread that loses the root resolves a different store than the test reads.
3. Green bar: those 4 files pass 3 runs in a row each; `cargo test -p boop-store -p boop-proc -p boop-harness --lib`,
   `cargo test -p boop --bin boop` stay green; boop2 contract suite
   (`cd /Users/chrishafley/projects/boop2-harmonize && BOOP_BIN=<built boop> bash tests/run.sh`) 0 not-ok.
4. REPORT.md: per failing test: cause (file:line), class (product/test/env), fix commit, evidence line.

## Rules
- Every test and boop invocation: sandboxed HOME. Never the real ~/.agent. The tests may create git worktrees and tmux
  sessions only inside their own scratch repos/sockets; if a test touches the hafley-rs repo's real worktree list or
  the default tmux socket, that is a bug to fix, not a thing to run around.
- No real model spend: llmock only. No Python. CARGO_BUILD_JOBS=4. Numbered Rust files `_N_name.rs`, no #[path].
- Commit per root cause; messages end `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`.
  Do not push. Do not install boop.
