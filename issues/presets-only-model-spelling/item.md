---
created: 2026-08-24
updated: 2026-08-25
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

- [x] `config presets` marks `gem37` DEAD with the bail text
- [x] `luna` spawns a codex lane with effort passed as config, never in the model string
- [x] a test spawns `--dry-run` for every preset and asserts the cmd parses

## Agent Runs

### 2026-08-25T04:32:33Z · @feat-identity-presets

cebcbc1 (branch feat/identity-presets, rebased onto main 8bbbd09).

`lane create --preset <name>` is the one spelling; `--model` and `--harness` are `hide = true` aliases over the same row. The preset table carries `harness`, `model`, `effort` (plus `variant`, `bin`) as separate fields; a legacy `name@effort` string still parses and is split at resolve time, so no `@medium` reaches a model string. Effort rides `SpawnSpec`/`ChannelSpec` beside the model and each harness spells it: codex `-c model_reasoning_effort=`, ACP `reasoning_effort` session option, acpx `name[effort]`, supervisor line `--effort`.

`boop config presets` runs `lane::preset_spawn_check` per row:

| row | harness | model | effort | status |
|---|---|---|---|---|
| luna | codex | gpt-5.6-luna | medium | ok |
| gem37 | opencode | openrouter/google/gemini-3.7-flash | | DEAD, "model ... is BANNED from opencode: its family runs on the `gemini` harness's flat-rate plan ..." |
| k3, k3-256k, kimi, kimi-fast | kimi | kimi-code/* | | ok |

The kimi rows are new: the kimi ACP agent takes only `kimi-code/*` ids and `kimi-*` derivation never matches a provider path, which is the case for harness being a field.

Receipt: `--preset luna` dry run prints `--model 'gpt-5.6-luna' --effort 'medium'` with no `@`. The real spawn (chore/luna-receipt in the scratchpad tree-repo) resolved an ACP session with no open_failed, committed e8dd214, and exited rc=0.

New test `crates/boop/tests/preset_dry_run.rs`: `lane create --dry-run` for all 25 rows, asserts the printed cmd parses under `sh -n`, the harness equals the row's field, effort is a flag and never an `@suffix`, and gem37 is refused by name.
