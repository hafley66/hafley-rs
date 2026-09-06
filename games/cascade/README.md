# Typed cascade receipt

```rust
use cascade::{GravityScale, Rules, GRAVITY_SCALE};

let mut rules = Rules::<bool>::default();
rules.set(GRAVITY_SCALE, GravityScale(1.0));
rules.when(|active, _| *active, (0, 1), GRAVITY_SCALE, GravityScale(0.0));
let frame = rules.resolve(&true, 0, GRAVITY_SCALE);
assert_eq!(frame.winner.unwrap().value, GravityScale(0.0));
```

Rules own typed declarations and selector function pointers. Resolution reads
one immutable caller-owned snapshot, orders matches by `(layer, specificity,
source_order)`, and returns their values and Rust call-site locations. The last
match wins; an empty match set has no winner. Selectors must read only their
arguments for deterministic replay.

The first property column is `GravityScale`, a dimensionless multiplier on the
caller's gravity. Its sole public key is `GRAVITY_SCALE`, ID 0. Key fields have
no public constructor, so an author cannot assign that ID to another Rust type.
The crate uses no type erasure, unsafe code, parser, linker, or serialized IR.
Additional property columns are outside this receipt.

Named caller: `kneeman-lines/6_recovery/content/rust/0_ship.rs`, compiled through
the `smash_sim` test target. It authors the V1 hull's zero-gravity rule, applies
the winner through the existing hull field, and calls the existing V1 reducer.
`content/rust/0_ship.test.rs` compares full serialized states and debug frames
over 90 input frames, restores a mid-tape snapshot, and checks the executable
V1 result. Shared rules and traces introduce no replacement world or reducer.

Verification on 2026-09-05:

- `cargo test -p cascade --locked`: 1 integration test and 2 doctests pass,
  including compile-time rejection of an array assigned to gravity scale.
- `cargo clippy -p cascade --all-targets -- -D warnings`: passes.
- `cargo check --workspace`: passes, with existing `boop::run_host` dead-code warning.
- `cargo test --workspace`: sandbox run fails on 6 existing local tmux tests.
  A run with local tmux access proceeds to `boop --test main`: 107 pass and
  `deliver_door::a_route_with_a_live_pane_takes_the_paste_rung` fails at
  `crates/boop/tests/deliver_door.rs:432` (`Mailbox` versus `PanePaste`). The same
  failure reproduces with the exact test run alone. That file is unchanged.
- `just --list`: passes. `just --fmt --check` reports existing interpolation
  spacing drift in the unchanged shared Justfile; it was not reformatted.
- Scoped `rustfmt --check` and `git diff --check`: pass.

The supporting API signatures, lifetimes, storage and identity conditions were
recorded before implementation in the caller's
`plans/1_astra_rust_first_game3.md`.
