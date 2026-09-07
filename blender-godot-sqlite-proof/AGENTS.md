# Lab exploration and testing

Visual test output is the backbone of this lab's exploration strategy.
For experiments with visible behavior, produce a short playable recording of the
actual implementation alongside deterministic machine-checked assertions.
Default to H.264 MP4 for compact playback; use GIF only when needed by the viewer.
Keep the recording command reproducible. Inspect rendered frames and verify the
encoded artifact before presenting it. State which behavior the test proves and
which integrations remain untested. Do not substitute an illustrative animation
for output captured from the implementation under test.

Before implementing a subsystem, inspect local vendors and domain-specific
libraries/tools for existing implementations. Start with
`research/10_smash_ecosystem.md` for fighter work. Record exact APIs, packaging,
dependencies, licenses, and verification gaps before writing overlapping code.
Distinguish published libraries, application internals, offline tools, and planned
features. Preserve the requested Rust simulation and SQLite boundaries; framework
discovery does not authorize a framework migration. Validate selected dependencies
with pinned-source fixtures and keep the visual test workflow above.
