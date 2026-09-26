---
created: 2026-09-26
updated: 2026-09-26
type: improvement
reporter: claude
status: open
priority: low
related: ['@cleave-cross-crate-reach']
labels: [extract]
---

# cleave output: default INFO flood, pre-existing orphan lines, untracked dirs in corpus

## Description

Corpus: ascii-renderer at main `1c24a4b` (single binary crate, `src/main.rs` declares ~140 `mod` lines), worktree `.claude/worktrees/agent-abab70b186ac7c270`. `ryi 0.1.0` = `~/.cargo/bin/ryi`, sha256 `c2ec8bb01142049e…`, installed 2026-09-26 03:24 -0400 from worktree `.claude/worktrees/merge-main` (since removed). 18 commits touched `crates/sprefa-extract` after that build (latest `0d686c37`); not re-checked against main. Found while planning `plans/3_engine_crate_isolation.md` (branch `plan/engine-crate-isolation-v3`, `ae4a003`), which splits an engine library crate out of the binary. All runs are dry runs.

Output noise from `ryi cleave` on a 140-module binary crate:

1. **INFO logs to stderr by default.** Each cleave dry run wrote 176–280 lines of `INFO extract_file{path=... lang="rust" bytes=...}: hafley_scm:`, one per corpus file. The plan is on stdout, but in a terminal (and in agent tool output) stderr buries it. Expected: default level WARN, with `-v` / `RUST_LOG` for per-file logs.
2. **"orphan" lines for untouched imports.** `cleave --drag src/opts.rs#rand_knob` printed 20 lines like `orphan automata from crate::automata` and `orphan Color from crossterm::style::Color`. These are the glob and unused imports already in `src/opts.rs`, not imports the cleave orphaned. Expected: report only imports that become unused because of this move, or put pre-existing ones under a flag.
3. **Untracked scratch dirs are in the corpus.** The planning agent reports that an untracked `.probe/` dir was scanned. Expected: honour `.gitignore`, and skip untracked files unless `--root` names them.

## Acceptance Criteria
- [ ] default log level hides per-file INFO
- [ ] `orphan` lists only imports orphaned by this plan
- [ ] corpus walk skips gitignored/untracked paths by default

## Comments

### 2026-09-26T22:28:00Z · @claude

Re-checked with ryi built from origin/main 7c51f866 (2026-09-26 18:26): all 7 dry-run outputs identical to the 03:24 binary except stage hashes. Still reproduces.
