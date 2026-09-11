# v1 combat and physics port

Source: `../kneeman-lines/0_rust_v1_ship/rust-sim/core/src/`. The source
directory has no Git metadata, so the first extraction receipt fingerprints every
consumed file before code moves.

## Boundary and type signatures

`game-combat` owns pure hit resolution. It does not own fighter state, input,
collision detection, rigid-body integration, rendering or content ingestion.

```rust
pub struct Strike {
    /** Authored hitbox base damage added to target percent.
     *  [target 40%] + [hit 12%] -> [target 52%] */
    pub damage: f32,
    /** Authored trajectory angle in degrees before Sakurai resolution and DI.
     *  [hitbox angle 32 deg] -> [trajectory 32 deg] */
    pub angle: f32,
    /** Authored base knockback (BKB) floor regardless of target percent.
     *  [BKB 30] -> [knockback min 30] */
    pub base_knockback: u32,
    /** Authored knockback growth (KBG) scaling factor with target percent.
     *  [target %] x [KBG 100] -> [scaled knockback] */
    pub knockback_growth: u32,
    /** Authored set knockback (WDSK) replacing ordinary percent in the KB formula.
     *  [WDSK > 0] replaces % -> f(WDSK, weight, KBG, BKB) */
    pub weight_dependent_set_knockback: u32,
}

pub struct Target {
    /** Target accumulated damage percent before hit resolution.
     *  [target 40%] + [strike 12%] -> [percent_after 52%] */
    pub percent: f32,
    /** Target character weight divisor dampening knockback acceleration.
     *  [target weight] + [hit terms] -> [knockback scaling] */
    pub weight: f32,
    /** Target stance selecting aerial vs grounded launch and Sakurai angle branch.
     *  [361 + grounded] -> [low-KB 0 deg / high-KB 44 deg]
     *  [361 + airborne] -> [45 deg] */
    pub grounded: bool,
}

pub struct DefenseInput {
    /** Sampled defense control stick coordinate vector [x, y] for DI angle offset.
     *  [stick perp component] + [trajectory angle] -> [rotated angle] */
    pub stick: [f32; 2],
}

pub struct Launch {
    /** Derived damage applied to target from the resolved strike.
     *  [strike 12%] -> [target damage +12%] */
    pub damage: f32,
    /** Derived knockback scalar magnitude computed from strike, percent, and weight.
     *  f(strike, target) -> [knockback 84.5] */
    pub knockback: f32,
    /** Derived initial launch velocity vector [vx, vy] along DI-adjusted angle.
     *  [kb 84.5] @ [angle 50 deg] -> [vx 54.3, vy 64.7] */
    pub velocity: [f32; 2],
    /** Derived freeze duration in frames applied to attacker and target actions.
     *  [strike damage] + [ruleset hitlag policy] -> [hitlag frames] */
    pub hitlag: u32,
    /** Derived hitstun duration in frames victim cannot act while launched.
     *  [knockback 84.5] -> [hitstun 33 frames] */
    pub hitstun: u32,
    /** Derived tumble state flag active when launch knockback exceeds threshold.
     *  [knockback] + [ruleset tumble threshold] -> [tumble bool] */
    pub tumble: bool,
}

pub fn resolve(strike: Strike, target: Target, input: DefenseInput) -> Launch;
```

The body is a fixed sequence:

```rust
let percent_after = target.percent + strike.damage;
let knockback = ssbm_utils::calc::knockback(/* strike + target values */);
let angle = ssbm_utils::calc::resolve_sakurai_angle(/* grounded */);
let angle = ssbm_utils::calc::apply_di(angle, input.stick.into());
let velocity = initial_velocity(knockback, angle, target.grounded);
Launch { damage, knockback, velocity, hitlag, hitstun, tumble }
```

The exact hitlag and tumble policies remain named ruleset inputs until their
source is qualified. No v1 numeric approximation silently becomes authority.

## Instance timeline

```text
input sample
  -> attacker action/frame
  -> bone-derived hit shape + target hurt shape
  -> Parry contact candidate
  -> game rule selects one Strike
  -> game-combat::resolve
  -> fighter reducer applies percent/hitlag/hitstun/tumble/velocity
  -> Rapier or deterministic kinematics advances durable bodies
  -> snapshot/checksum
  -> corrected presentation rows after replay catch-up
```

Hitlag freezes action clocks before ordinary fighter integration. DI reads the
victim's input for the resolved hit. Hitstun decrements on advancing simulation
ticks. Contact suppression prevents one active hitbox from applying repeatedly
to the same target until its authored refresh rule permits it.

## Storage, reads and writes

Immutable shared content:

- attack definitions and frame windows;
- character weight and ruleset constants;
- skeleton, animation and attachment definitions;
- content and ruleset identity hashes.

Snapshot-owned state:

- fighter percent, velocity, hitlag, hitstun and tumble;
- action/frame clocks and input history;
- per-hitbox/per-target contact suppression;
- dynamic physics state that changes future ticks.

Derived state:

- bone matrices and Parry shapes rebuilt from immutable content plus restored
  action/frame state;
- SQLite rows and renderer buffers republished after replay catch-up.

Uniqueness is `(attacker entity, attack instance, hitbox id, target entity)`.
Entity IDs remain stable across snapshots. A presentation row never authorizes a
hit or feeds the reducer.

## Port sequence and gates

### C1: freeze v1 behavior

Fingerprint `combat.rs`, `physics.rs`, `moves/mod.rs`, `fighter.rs`, `step.rs`
and the relevant v1 tests. Convert selected v1 scenarios into data-first golden
vectors covering damage, fixed/growth knockback, DI angle, hitlag, hitstun,
tumble, shielding and repeated-contact suppression.

Terminal condition: committed fingerprints plus golden inputs/outputs execute
against the untouched v1 code. Every copied behavior names its v1 source symbol.

### C2: extract pure combat resolution

Create `crates/combat` using `ssbm_utils` for its available knockback, Sakurai
angle, DI, velocity and hitstun functions. Preserve unsupported v1 behavior as
explicit rule inputs or unresolved cases.

Terminal condition: native and `wasm32-unknown-unknown` tests reproduce the C1
vectors; the crate has no Godot, Rapier, SQLite or renderer dependency.

TC39 exit: `game-combat` advances stage 1 to 2.

### C3: integrate fighter damage states

Extend `game-fighter` with hitlag, hitstun, tumble and launch response. Replace
the Pigeon sandbag-only application path with the shared resolver while retaining
the sandbag as a receiver fixture. Drive DI through quantized `game-input`.

Terminal condition: attack, hitlag, DI, launched movement and recovery tapes
restore at each transition and replay to identical state/checksums.

TC39 exit: `game-combat` advances stage 2 to 2.7. `game-fighter` stays at 2.7
until its existing source-fidelity gates also pass.

### C4: qualify contact and physics adapters

Derive world-space collision shapes from action/frame/bone state. Use Parry for
contact candidates. Keep Rapier behind the existing durable-physics boundary and
compare it with deterministic kinematic fixtures for launch, landing and moving
support.

Terminal condition: Pigeon versus Dog, sandbag, stage landing and rollback-burst
fixtures pass natively and across native/WASM peers. Allocation and tick timing
receipts name tracing configuration.

TC39 exit: `game-combat` advances stage 2.7 to 3. A future `game-physics`
package is proposed only if C4 produces a reusable API independent of Smash.

### C5: remove duplicate combat paths

Move remaining consumers to `game-combat`, retain provenance fixtures, and
delete superseded v1-derived implementations only after equivalence tests pass.

Terminal condition: one combat resolver remains in the active dependency graph;
`just status` derives current combat gates from executable exports and
source-fingerprinted receipts.

