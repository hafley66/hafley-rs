set shell := ["bash", "-cu"]

soopy-scale repo pathspec="" handles="500" batch="16" label="manual":
    bash crates/soopy/bench/0_run.sh "{{repo}}" "{{handles}}" "{{batch}}" "{{label}}" "{{pathspec}}"

soopy-scale-linux-deps handles="500" batch="16":
    bash crates/soopy/bench/0_run.sh /Users/chrishafley/projects/ext/linux "{{handles}}" "{{batch}}" linux-deps ':(glob)**/Cargo.toml' ':(glob)**/package.json' ':(glob)**/go.mod' ':(glob)**/Makefile' ':(glob)**/Kconfig'

soopy-scale-linux-all handles="500" batch="16":
    bash crates/soopy/bench/0_run.sh /Users/chrishafley/projects/ext/linux "{{handles}}" "{{batch}}" linux-all

test-git-optional:
    cargo test -p soopy --test 6_git_optional

perf-git-status-smoke:
    bash crates/soopy/bench/1_git_status.sh smoke

perf-git-status repo:
    bash crates/soopy/bench/1_git_status.sh repo "{{repo}}"
