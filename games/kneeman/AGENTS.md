# Game3 working scope

Read README.md and docs/2_next.md. This directory is the active Kneeman game.
Use its justfile. app/ owns the Godot application; src/ owns the simulation.

- Project M is the gameplay target. Name exact source versions in behavior receipts.
- Preserve the ship, drawn/SVG items, stages, destruction, debugger and friend-photo art workflow.
- Libraries live in the containing repository's crates/. Game rules and tuning belong here.
- Use the existing fixed-tick simulation and replay/rollback interfaces. Tie architecture changes
  to a concrete missing mechanic and an executable input sequence before expanding abstractions.
- Old checkouts are read-only references. Open the relevant implementation for one mechanic or
  tool; do not import their broad plans, architectural mandates or alternative runtimes.
- Maintain one boot scene and one production runtime. /game3/ is the publish target; /game/ stays intact.
- Use maintained physics/geometry dependencies for new solver work. Do not replace libraries
  with handwritten substitutes as part of a cleanup.
- Run just game-test for gameplay/shell changes, just art-test for imports, and just web-check for
  deployment changes. Validate the real web export before publishing. Report skipped gates.
- Keep captured photos, Steam credentials and local art caches out of Git. Importing a new roster
  entry must preserve existing entries. Captures are local/download-only until upload is authorized.
- Update docs/2_next.md with evidence and the next bounded task. An implemented feature and a
  behavior proven equivalent to Project M are separate ledger statuses.
