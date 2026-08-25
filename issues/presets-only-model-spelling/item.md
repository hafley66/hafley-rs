---
created: 2026-08-24
updated: 2026-08-24
type: improvement
status: open
priority: high
epic: boop-one-path
labels: [domain-boop]
size: M
---

# Presets are the only model spelling; harness and bail derive from the table

## Description

## Description

Three spellings: `--preset`, bare `--model`, `--harness` override. `luna` preset = `gpt-5.6-luna@medium`, rejected by the codex ACP agent (02:10:31 open_failed). `gem37` preset exists but the opencode bail refuses it and no gemini harness exists. Both found only at spawn.

Cut: `lane create --preset <name>` is the only spelling; `--model`/`--harness` hidden aliases; the preset table carries harness, model, effort as separate fields; `boop config presets` runs the bail and prints DEAD for any preset that cannot spawn.

## Acceptance Criteria

- [ ] `config presets` marks `gem37` DEAD with the bail text
- [ ] `luna` spawns a codex lane with effort passed as config, never in the model string
- [ ] a test spawns `--dry-run` for every preset and asserts the cmd parses
