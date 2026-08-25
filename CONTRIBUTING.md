# contributing

## commits

release automation reads conventional commit subjects.

```text
feat(boop): add an event source
fix(boop): handle a missing finish event
perf(boop-mux): reduce tmux queries
refactor(boop): separate event decoding
feat(boop)!: change the public event model
```

`feat`, `fix`, `perf`, and `refactor` enter the changelog and request a release. use `!` or a `BREAKING CHANGE:` footer for an incompatible public api change. `docs`, `test`, `build`, `ci`, `chore`, and `style` do not request a release.

## versions

`boop` and `boop-mux` share the `boop` release-plz version group. a change affecting either package can advance the group version. while the packages use `0.0.x`, release-plz advances features and fixes by one patch version.

release-plz owns release-time edits to these fields and files:

- `crates/boop/Cargo.toml` package version
- `crates/boop-mux/Cargo.toml` package version
- internal dependency versions between the packages
- `Cargo.lock`
- `CHANGELOG.md`

releases run locally with `just release`: `release-plz update` writes the version bumps and changelog, the recipe commits and pushes them, and `release-plz release` creates package tags and github releases using the `gh` login. these crates use git-only releases and are not published to crates.io. no actions workflow releases anymore.

## local checks

```sh
just test          # cargo nextest run --workspace, the fast path
just test-ci       # cargo test --workspace --locked, exactly what CI runs
```

`just test` needs `cargo-nextest`; `cargo install cargo-nextest --locked`
installs it. CI runs `cargo test`, so a change has to pass `just test-ci`
before it ships.

pull requests also compare the public rust api of `boop` and `boop-mux` with the pull request base commit using cargo-semver-checks.

## repository setting

releasing needs a `gh auth login` with `repo` scope on the releasing machine. no cargo registry token and no actions permissions are required.
