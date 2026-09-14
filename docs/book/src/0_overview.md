# Overview

hafley-rs is a Cargo workspace. The main deliverable is `boop`, a CLI that
spawns coding agents (claude, codex, opencode, kimi) into git worktrees, mails
them, waits on them, and reads every transcript on the machine as one SQLite
store.

Around that CLI sit the crates it is built from: process control, the
per-harness transcript formats, the ACP lane channel, the tmux seam, the
relational store, and a shared tracing configuration. `soopy` is a separate
crate in the same workspace for Git revision and filesystem source
enumeration. `boop-turnvis` ports the terminal turn matcher from TypeScript.

## What boop does

- Spawn an agent into its own git worktree and tmux pane (`beep lane create`).
- Mail any registered agent and block for its answer (`beep`, `wait`).
- Read every transcript on the machine (claude, codex, opencode, kimi) as rows
  in one SQLite store at `~/.agent/boop.db` (`db`).
- Ask what went wrong without opening a log (`debug`).

`boop --help` is the usage contract; every flag is in it.

## Where to go next

| chapter | content |
| --- | --- |
| [Getting started](1_getting-started.md) | toolchain, build, install, and test commands |
| [Crate map](2_crate-map.md) | what each workspace crate owns and how they depend |
| [Building these docs](3_docs-workflow.md) | how the book and the API reference are built and published |
| [API reference](4_api-reference.md) | the generated rustdoc for every crate |
