set shell := ["bash", "-cu"]

boop-perf-grid:
    cargo test -p boop --test bench_grid -- --nocapture

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

test-source-mutations-commit:
    cargo test -p soopy --test 14_commit_engine -- --nocapture

test-source-mutations:
    cargo test -p soopy --test 9_source_actions --test 10_edit_producers --test 11_mutation_planner --test 12_producer_planner --test 13_stage_store --test 14_commit_engine --test 15_source_mutations -- --test-threads=1

perf-source-mutations files="1000" edits_per_file="100" bytes_per_file="4096" receipt="target/perf-source-mutations/receipt.json":
    cargo run --quiet --release -p soopy --example 5_source_mutations_scale -- --files "{{files}}" --edits-per-file "{{edits_per_file}}" --bytes-per-file "{{bytes_per_file}}" --receipt "{{receipt}}"

perf-source-mutations-commit files="1000":
    cargo run -q -p soopy --example 4_commit_scale -- --files "{{files}}"

test-git-optional:
    cargo test -p soopy --test 6_git_optional

perf-git-status-smoke:
    bash crates/soopy/bench/1_git_status.sh smoke

perf-git-status repo:
    bash crates/soopy/bench/1_git_status.sh repo "{{repo}}"
