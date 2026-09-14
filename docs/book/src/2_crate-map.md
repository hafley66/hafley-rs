# Crate map

Nine crates make up the workspace. `crates/*` is the workspace member glob in
the root `Cargo.toml`.

| crate | job | depends on (in workspace) |
| --- | --- | --- |
| `boop` | the CLI: `beep`, `wait`, `debug`, `db`, `tui`; the help text is the usage contract. Also a library facade over the crates below. | boop-proc, boop-harness, boop-acp, boop-store, hafley-observe |
| `boop-proc` | process control: lane spawn, supervision, parent policy, the lane mailbox, and the embeddable coroutine host | boop-acp, boop-harness, boop-store |
| `boop-harness` | per-harness transcript formats, session roots, the identity ladder, and the worktree a spawn runs in | boop-acp, boop-store |
| `boop-acp` | the typed lane channel: one ACP client per agent conversation, or a tmux TUI driver where there is no ACP door | boop-store |
| `boop-store` | the relational store over `~/.agent/boop.db`, its schema and migrations, and the transcript projection that fills it | boop-mux |
| `boop-mux` | the tmux multiplexer seam: one `Multiplexer` trait, one `Tmux` implementation | none |
| `hafley-observe` | shared executable-owned tracing configuration for the binaries | none |
| `soopy` | Git revision and filesystem source enumeration, reading, and watching | hafley-observe |
| `boop-turnvis` | terminal turn matcher ported from TypeScript, checked against a golden fixture corpus | none |

## Dependency order

The dependency edges form two groups.

- The boop chain, in order: `boop-mux` -> `boop-store` -> `boop-acp` ->
  `boop-harness` -> `boop-proc` -> `boop`. Each crate only depends on the ones
  before it.
- `hafley-observe` is independent. Both `boop` and `soopy` depend on it.
  `soopy` depends on nothing else in the workspace.
- `boop-turnvis` depends on nothing in the workspace.

The channel crate sits below the harness adapters because each adapter's
`spawn` constructs its own channel, so `boop-harness` depends on `boop-acp`
rather than the other way around.

## Documentation coverage

`cargo doc --workspace --no-deps --locked` documents the library and binary
targets of every crate that its default features enable. Examples, integration
tests, and build scripts are not documented (their targets carry `doc = false`),
and a target whose `required-features` the default feature set does not enable
is not built. The coverage checker lists every excluded target with its reason
rather than dropping it silently.

Rustdoc names an output directory after the crate name, so a hyphen becomes an
underscore (`boop-acp` documents to `boop_acp`). A crate whose library and
binary share a name documents to one directory; the landing page shows that
directory once with the combined kind. The generated page at `api/index.html`
is built from the workspace membership and the pages the doc build actually
emitted.
