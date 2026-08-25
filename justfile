set shell := ["bash", "-cu"]

# Install boop over ~/.cargo/bin/boop from a clean tree whose HEAD is already on
# origin/main, stamping that sha into `boop --version`. Refuses otherwise.
install-boop:
    bash crates/boop/scripts/install.sh

# The check on its own, for a driver reading a tree before it builds.
install-boop-check repo=".":
    bash crates/boop/scripts/install-guard.sh "{{repo}}"

# Version bumps + changelog from conventional commits since the last tags,
# committed and pushed, then tags + GitHub releases minted from this machine
# (gh supplies the token). The Actions release flow is retired.
release:
    release-plz update
    if ! git diff --quiet; then git add CHANGELOG.md Cargo.lock crates/*/Cargo.toml && git commit -m "chore(release): version bumps and changelog" && git push origin main; fi
    release-plz release --git-token "$(gh auth token)"

boop-perf-grid:
    cargo test -p boop-harness --test bench_grid -- --nocapture

soopy-scale repo pathspec="" handles="500" batch="16" label="manual":
    bash crates/soopy/bench/0_run.sh "{{repo}}" "{{handles}}" "{{batch}}" "{{label}}" "{{pathspec}}"

soopy-scale-linux-deps handles="500" batch="16":
    bash crates/soopy/bench/0_run.sh /Users/chrishafley/projects/ext/linux "{{handles}}" "{{batch}}" linux-deps ':(glob)**/Cargo.toml' ':(glob)**/package.json' ':(glob)**/go.mod' ':(glob)**/Makefile' ':(glob)**/Kconfig'

soopy-scale-linux-all handles="500" batch="16":
    bash crates/soopy/bench/0_run.sh /Users/chrishafley/projects/ext/linux "{{handles}}" "{{batch}}" linux-all

perf-source-mutations-planner files="1000" edits_per_file="100" bytes_per_file="4096":
    bash crates/soopy/bench/2_source_mutations_planner.sh "{{files}}" "{{edits_per_file}}" "{{bytes_per_file}}"

perf-source-mutations-stage files="1000" edits_per_file="100" bytes_per_file="4096" store="target/soopy-stage-scale":
    cargo run -q -p soopy --example 3_stage_store_scale -- --files "{{files}}" --edits-per-file "{{edits_per_file}}" --bytes-per-file "{{bytes_per_file}}" --store "{{store}}"

# The whole gate. nextest runs each test as its own process across one pool, so
# test binaries overlap; cargo test finishes one before it starts the next.
test *ARGS:
    cargo nextest run --workspace {{ARGS}}

# What CI runs, for reproducing a CI-only failure locally.
test-ci:
    cargo test --workspace --locked

test-source-mutations-commit:
    cargo test -p soopy --test main -- t14_commit_engine --nocapture

test-source-mutations:
    cargo test -p soopy --test main -- --test-threads=1 t9_source_actions t10_edit_producers t11_mutation_planner t12_producer_planner t13_stage_store t14_commit_engine t15_source_mutations

perf-source-mutations files="1000" edits_per_file="100" bytes_per_file="4096" receipt="target/perf-source-mutations/receipt.json":
    cargo run --quiet --release -p soopy --example 5_source_mutations_scale -- --files "{{files}}" --edits-per-file "{{edits_per_file}}" --bytes-per-file "{{bytes_per_file}}" --receipt "{{receipt}}"

perf-source-mutations-commit files="1000":
    cargo run -q -p soopy --example 4_commit_scale -- --files "{{files}}"

test-git-optional:
    cargo test -p soopy --test 6_git_optional

test-soopy-multi-repo-refresh:
    cargo test -p soopy --test main -- t16_multi_repo_refresh --nocapture

perf-soopy-multi-repo-refresh repositories="32" rounds="3" concurrency="4":
    cargo run --release -q -p soopy --example 6_multi_repo_refresh -- --repositories "{{repositories}}" --rounds "{{rounds}}" --concurrency "{{concurrency}}"

perf-git-status-smoke:
    bash crates/soopy/bench/1_git_status.sh smoke

perf-git-status repo:
    bash crates/soopy/bench/1_git_status.sh repo "{{repo}}"

# Bring a worktree to the level a lane needs before it edits: the crate index
# fetched, the boop test binaries built into the target every worktree shares.
boop-start:
    #!/usr/bin/env bash
    set -euo pipefail
    started=$SECONDS
    cache="${BOOP_START_CACHE:-$HOME/.cache/boop}"
    shared="${BOOP_CARGO_TARGET_DIR:-$cache/cargo-target}"
    export CARGO_TARGET_DIR="$shared"
    cargo fetch --quiet
    cargo build -p boop --tests --quiet
    echo "boop-start: cargo fetch and boop tests into $shared, $((SECONDS - started))s"
