# Mechanics kernel compass

This directory is the closed, content-neutral mechanics language implemented by Rust.

## North star

Rust matches **operations and capabilities**. Programs match **identity**.

Allowed Rust branch:

```rust
match op {
    Op::Spawn(..) => ...,
    Op::ApplyImpulse(..) => ...,
    Op::SplitGeometry(..) => ...,
}
```

Rejected Rust branch:

```rust
match kind {
    SomeSpecificCharacter => ...,
    SomeSpecificItem => ...,
    SomeSpecificMaterial => ...,
}
```

## Placement rules

- Immutable definitions live in `Program` and are content-hashed.
- Cross-frame residue lives in bounded `ScriptState`/`SimState` fields.
- Same-frame computation is transient.
- Cross-entity gameplay changes are commands.
- Output-only work is a confirmation-gated shell effect.
- Geometry/physics/graph algorithms are native kernels over generic types.
- Prefab trees are authoring structure; runtime storage may remain flat and specialized.
- CSS-style cascade computes values. It never performs writes.
- Loaded documents declare named rule sets. Runtime `ActiveSets` is ordinary rollback collection
  state; selectors test membership and events may add/remove sets without reloading the document.
- Cross-entity structure is a flat table of typed directed relations, not scene-tree ancestry.
- `MechanicsDocument` is immutable loaded content: addressed programs, prefab definitions, rule-set
  names, and portable cases. It is not rollback state.
- `EntityRecord` is the rollback handle for one interpreted composition. `Recompose` preserves its
  `EntityId` and external graph facts while replacing prefab, kind, program, and prefab-local state.
- Relations expose ordinary collection writes: `AddRelation`/`RemoveRelation` for the edge set,
  and `SetRelationIfEmpty`/`SetRelation` for a slot keyed by `(kind, to)`.

## Language laws

1. Snapshot reads, transactional writes.
2. Explicit phase and priority.
3. Closed operation vocabulary.
4. No arbitrary callbacks or reflection.
5. No strings or hash iteration in the frame interpreter.
6. Every tape, register bank, and event cascade is bounded.
7. Content IDs are data IDs, not Rust identity enums.
8. Source provenance survives normalization.
9. Netplay freezes one canonical program hash.
10. A new content kind using existing capabilities changes zero Rust files.
11. Programs ship pure data-driven cases: initial state + events -> state and command trace.

## Test doctrine

`LanguageCase` is the executable specification format. A case supplies one serializable initial
instance, an event sequence, and data expectations over final fields, exact command positions, or
emitted events. `run_case` calls the real planner and committer; it is not a mock interpreter.

`GraphCase` is the world-level companion. It adds initial active rule sets and typed relations,
bounded world commit, and expectations over set membership, connections, and outcome events. Set
changes take effect on the next planning snapshot; they do not reinterpret commands already planned
this frame.

`DocumentCase` is the full document/entity companion. It spawns addressed prefabs into a bounded
entity table, dispatches each event through the entity's current program, and commits entity/world
facts atomically. Use it for hot recomposition, multi-prefab, and eventually native/interpreted
differential fixtures. External authored fixtures belong here; a Rust builder is not proof of the
load boundary.

Use exact command assertions when ordering is the behavior under test. Prefer field/emission
expectations for ordinary mechanic intent so harmless trace additions do not churn every fixture.
Collision, spawn, and full multi-entity state join this format when the interpreter reaches
`SimState`. Outcome events are recorded but do not recursively cascade until ordering and a fixed
cascade budget are explicitly fixed.

## Native vocabulary test

A Rust addition is justified only when at least one is true:

- it implements a new reusable algorithm;
- it exposes a new operation over existing internal types;
- it introduces a genuinely new bounded storage class;
- it adds a shell capability unavailable through existing effects.

Otherwise it is program data.

## Vector-body doctrine

The generic concept is geometry + body + collider + material + paint + program + optional topology.
No engine primitive is named after ink, a tetromino, a ship, or a particular weapon. Those identities
are prefabs/programs assembled from vector paths, rigid bodies, contacts, anchors, meters, and events.

GPU resources never enter rollback state. Gameplay collision uses canonical Rust geometry; shells
cache their own render representation by geometry identity/revision and paint.

## Migration

Existing native `ItemBehavior`, `DrawTool`, and `SpecialKind` implementations may coexist with the
interpreter. Do not rewrite working mechanics in one pass. Port behind differential tests and delete
identity branches only after parity.
