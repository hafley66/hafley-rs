---
created: 2026-08-24
updated: 2026-08-25
type: bug
status: open
priority: high
epic: boop-one-path
labels: [domain-boop]
size: S
---

# kimi transcript projection drops tool and assistant bodies

## Description

## Description

`crates/boop-harness/src/harness/kimi.rs` `project_line` writes `""` for tool turns (line 508) and some assistant turns (line 437), same defect class as opencode (59582fa) and codex (fix/codex-tool-bodies). `boop debug chore-kimi-probe` section 4 printed `assistant none / tool none` for a lane that ran 2 bash calls. Fixture: ~/.kimi-code/sessions/wd_kimi-probe_79336ba0c1a4/session_8fbd623a-*/agents/main/wire.jsonl (parts: `think`, `display.command`, tool result events).

## Acceptance Criteria

- [x] tool turns carry name + command + result text
- [x] assistant turns carry text; `think` parts kept as body with a `[think]` prefix
- [x] fixture test: zero empty tool and assistant bodies
- [x] unknown part kinds keep raw JSON

## Agent Runs

### 2026-08-25T12:55:01Z · @feat-epic-wave-b

93490ff kimi project_line: tool.call projects name+args (2000 char cap), a new tool.result arm projects output or its joined text parts (4000 char cap, 'error:' tag when isError), content.part projects every kind (text bare, others tagged, key read off the kind, raw JSON as the floor), and a leading usage.record writes '[usage] <model>' instead of ''. Fixture crates/boop-harness/tests/fixtures/kimi/wd_kimi-probe/session_8fbd623a/agents/main/wire.jsonl, 16 lines trimmed from the real chore-kimi-probe transcript. 11 new tests including the receipt the_probe_fixture_keeps_every_tool_and_assistant_body (0 empty tool bodies, 0 empty assistant bodies). cargo test -p boop-harness 124 passed 0 failed 1 ignored; full suite across boop/boop-acp/boop-harness/boop-proc/boop-store/boop-mux green.
