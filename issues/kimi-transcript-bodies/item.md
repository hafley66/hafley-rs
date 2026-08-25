---
created: 2026-08-24
updated: 2026-08-24
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

- [ ] tool turns carry name + command + result text
- [ ] assistant turns carry text; `think` parts kept as body with a `[think]` prefix
- [ ] fixture test: zero empty tool and assistant bodies
- [ ] unknown part kinds keep raw JSON
