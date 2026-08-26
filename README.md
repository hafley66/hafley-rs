# hafley-rs

A Cargo workspace. The main deliverable is `boop`, a CLI that spawns coding
agents (claude, codex, opencode, kimi) into git worktrees, mails them, waits on
them, and reads every transcript on the machine as one SQLite store.

## Crates

| crate | job |
| --- | --- |
| [`boop`](crates/boop) | the CLI: `beep`, `wait`, `debug`, `db`, `tui`; the help text is the usage contract |
| [`boop-proc`](crates/boop-proc) | lane spawn, supervision, parent-death policy, the lane mailbox |
| [`boop-harness`](crates/boop-harness) | per-harness transcript formats, session roots, the identity ladder, the worktree a spawn runs in |
| [`boop-acp`](crates/boop-acp) | one ACP client per agent conversation |
| [`boop-store`](crates/boop-store) | the SQLite schema at `~/.agent/boop.db`, migrations, transcript bytes to rows |
| [`boop-mux`](crates/boop-mux) | the tmux seam: one trait, one implementation over `tmux_interface` |
| [`hafley-observe`](crates/hafley-observe) | one tracing configuration shared by the binaries |
| [`soopy`](crates/soopy) | git revision and filesystem source enumeration, reading, watching |

## Install

```bash
cargo install --path crates/boop --force
boop --help
```

`just install-boop` does the same from a clean tree on `origin/main` and stamps
the sha into `boop --version`.

## Develop

```bash
just test                 # cargo nextest run --workspace
just test-ci              # cargo test --workspace --locked
just release              # release-plz: bump, changelog, tag, GitHub release
```

Releases are tagged `boop-vX.Y.Z` and `boop-mux-vX.Y.Z`; the changelog is
[CHANGELOG.md](CHANGELOG.md). Contribution rules are in
[CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT or Apache-2.0, at your option. See [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE).
