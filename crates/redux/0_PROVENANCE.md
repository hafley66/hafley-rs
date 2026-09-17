# Redux copy provenance

- Source repository: `/Users/chrishafley/projects/hafley-games`
- Source revision: `80ded24fb6cfb664a73b3fb10d3463e1a7413cff`
- Source path: `crates/redux`
- Destination: `hafley-rs/crates/redux`
- Copy basis: files tracked by Git at the source revision

The destination preserves every tracked source, example, UI fixture, diagram,
and supporting document except the source crate's nested `Cargo.lock`. The
workspace member uses the `hafley-rs` root lockfile.

Workspace integration removes the nested `[workspace]` marker from
`Cargo.toml`. The package name, version, edition, license, publish policy,
library dependencies, feature names, and reusable library sources are unchanged.

The destination omits the source-only `rollback` path dev-dependency and its
`ggrs` dev-dependency. Accordingly, `tests/2_statechart.rs` omits `ChartSim` and
these cross-crate integration tests:

- `ggrs_requests_restore_chart_and_replay_full_state_and_effects`
- `ggrs_restore_detects_accidental_serde_reinitialization`

The remaining statechart tests and registry-backed dev-dependencies are retained.
The source working tree's untracked `examples/epic_loop.rs` was not copied.
