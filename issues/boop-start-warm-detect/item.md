---
created: 2026-08-19
updated: 2026-08-19
type: feature
status: open
priority: high
related: ['@boop-doa-lane-carcass']
---

# boop-start: detect, run, and tell the agent setup is done so no model re-derives 'how to get started'

## Description

## Description

Chris 2026-08-19: "i want just boop-start detection for worktree'ing and never have smaller model question how it needs to get started, it happens a bunch."

Today: `worktree.rs:78-105 warm_start` runs `just boop-start` in a fresh lane worktree when the recipe exists (`has_recipe`, `just --show boop-start`), bounded by `SPAWN_CHILD_TIMEOUT` since #29. sprefa has the recipe (`justfile:191`, extractor build + node_modules). hafley-rs has none. Nothing tells the agent the warm-up ran, what it did, or that it may skip setup, so smaller models re-derive setup from README and ask.

## Acceptance Criteria

- [ ] Every repo boop spawns into has a `boop-start` recipe or boop says so at spawn time in one line (`boop-start: no recipe in <repo>, nothing to warm`); hafley-rs gets a recipe (`cargo fetch`, `cargo build -p boop --tests` into the shared target) in this PR.
- [ ] The warm-up's outcome is written into the lane's brief preamble / first injected line: `boop-start: ready in Ns (<the recipe's own summary lines>)`, and one sentence the agent reads: "setup is done; do not run installs or builds to 'get started'; build only what you change." Same line for `boop beep agent register` natives when they report a worktree.
- [ ] `boop beep lane create --dry-run` prints whether boop-start will run and from which justfile.
- [ ] A fresh worktree with the recipe and a stale shared target runs boop-start once; a second spawn into a sibling worktree reuses the shared target (COUNT test: cargo/pnpm invoked once across two spawns).
- [ ] Test: spawn into a repo with no recipe, assert the one-line notice and no error.
