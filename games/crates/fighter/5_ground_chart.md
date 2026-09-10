# Ground statechart (generated)

Source: `src/1b_ground.rs`, rendered from `ground::decide(Phase, Event)`.
Inventory: 12 phases × 3 event variants × 128 assignments of the seven `Facts`
booleans. `JumpRequest` is evaluated across all assignments to verify that it
is independent of facts.

`Handled` means `decide` returned `None`: the phase and its clock are preserved.
Solid edges are returned `Some(destination)` decisions. A self-edge is a real
self-transition, including the dash-age reset documented in `1b_ground.rs`.
Witness counts show the exhaustive assignments represented by each edge.
This chart covers the current grounded local policy. Physics integration,
air/contact behavior and the broader source inventory remain outside this chart.

```mermaid
stateDiagram-v2
direction LR
    Handled: handled / no transition
    Idle
    Walk
    Dash
    Run
    Brake
    Turn
    Squat
    Crouch
    Landing
    Jump
    Fall
    AirJump
    Idle --> Squat: JumpRequest (128/128 facts)
    Idle --> Walk: GroundIntent [!dash && walk && !down] (16/128 facts)
    Idle --> Dash: GroundIntent [dash && !down] (32/128 facts)
    Idle --> Crouch: GroundIntent [down] (64/128 facts)
    Idle -."GroundIntent [!dash && !walk && !down] (16/128 facts)".-> Handled
    Idle -."Motion [true] (128/128 facts)".-> Handled
    Walk --> Squat: JumpRequest (128/128 facts)
    Walk -."GroundIntent [true] (128/128 facts)".-> Handled
    Walk --> Idle: Motion [!dash && !walk] (32/128 facts)
    Walk --> Dash: Motion [dash] (64/128 facts)
    Walk -."Motion [!dash && walk] (32/128 facts)".-> Handled
    Dash --> Squat: JumpRequest (128/128 facts)
    Dash --> Dash: GroundIntent [dash && !down] (32/128 facts)
    Dash --> Crouch: GroundIntent [down] (64/128 facts)
    Dash -."GroundIntent [!dash && !down] (32/128 facts)".-> Handled
    Dash --> Dash: Motion [reverse] (64/128 facts)
    Dash --> Run: Motion [forward && !reverse && finished] (16/128 facts)
    Dash --> Brake: Motion [!forward && !reverse] (32/128 facts)
    Dash -."Motion [forward && !reverse && !finished] (16/128 facts)".-> Handled
    Run --> Squat: JumpRequest (128/128 facts)
    Run --> Dash: GroundIntent [dash && !down] (32/128 facts)
    Run --> Crouch: GroundIntent [down] (64/128 facts)
    Run -."GroundIntent [!dash && !down] (32/128 facts)".-> Handled
    Run --> Brake: Motion [!walk && !reverse] (32/128 facts)
    Run --> Turn: Motion [reverse] (64/128 facts)
    Run --> Crouch: Motion [walk && !reverse && down] (16/128 facts)
    Run -."Motion [walk && !reverse && !down] (16/128 facts)".-> Handled
    Brake --> Squat: JumpRequest (128/128 facts)
    Brake -."GroundIntent [true] (128/128 facts)".-> Handled
    Brake --> Idle: Motion [stopped] (64/128 facts)
    Brake -."Motion [!stopped] (64/128 facts)".-> Handled
    Turn --> Squat: JumpRequest (128/128 facts)
    Turn -."GroundIntent [true] (128/128 facts)".-> Handled
    Turn --> Idle: Motion [!reverse && finished] (32/128 facts)
    Turn --> Dash: Motion [reverse] (64/128 facts)
    Turn -."Motion [!reverse && !finished] (32/128 facts)".-> Handled
    Squat -."JumpRequest (128/128 facts)".-> Handled
    Squat -."GroundIntent [true] (128/128 facts)".-> Handled
    Squat --> Jump: Motion [finished] (64/128 facts)
    Squat -."Motion [!finished] (64/128 facts)".-> Handled
    Crouch --> Squat: JumpRequest (128/128 facts)
    Crouch -."GroundIntent [true] (128/128 facts)".-> Handled
    Crouch --> Idle: Motion [!down] (64/128 facts)
    Crouch -."Motion [down] (64/128 facts)".-> Handled
    Landing -."JumpRequest (128/128 facts)".-> Handled
    Landing -."GroundIntent [true] (128/128 facts)".-> Handled
    Landing --> Idle: Motion [finished] (64/128 facts)
    Landing -."Motion [!finished] (64/128 facts)".-> Handled
    Jump -."JumpRequest (128/128 facts)".-> Handled
    Jump -."GroundIntent [true] (128/128 facts)".-> Handled
    Jump -."Motion [true] (128/128 facts)".-> Handled
    Fall -."JumpRequest (128/128 facts)".-> Handled
    Fall -."GroundIntent [true] (128/128 facts)".-> Handled
    Fall -."Motion [true] (128/128 facts)".-> Handled
    AirJump -."JumpRequest (128/128 facts)".-> Handled
    AirJump -."GroundIntent [true] (128/128 facts)".-> Handled
    AirJump -."Motion [true] (128/128 facts)".-> Handled
```
