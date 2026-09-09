# Smash application destination

Approved destination: one Rust application crate here, no nested crates.
Reusable first-party packages belong in `../crates/`.

Status: destination recorded; Cargo application and consumer migration are A1
in [the domain task ledger](../3_tasks.md). No runnable game is claimed here.
Start from the existing Falcon slice and its verification; do not create a second
simulation implementation or another replacement demo.

First playable gate: walking, jumping, crouching, directional attack/special input
routing, side smash, down tilt, neutral air, fair and bair on Small Battlefield.
"Crouch tilt" is provisionally interpreted as down tilt. Direction is relative to
facing; exact PM thresholds/windows and special move coverage need source evidence.
Record actual state in an MP4 and replay the input tape to identical authoritative
state. Full PM3.6 Falcon remains the larger target. Stocks, respawn and the complete
two-fighter match are on deck after this gate.

See [living architecture](../1_roadmap.svg) and [rollback ownership](../2_roadmap.md).
