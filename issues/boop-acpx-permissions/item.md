---
created: 2026-08-28
updated: 2026-08-28
type: bug
reporter: codex
assignee: opus
status: resolved
priority: high
labels: [boop, acpx]
---

# Boop ACPX coordinators deny delegated file permissions

## Description

`boop --preset glm53f --name <route>` registers an ACPX/OpenCode coordinator, then the first prompt exits with status 5 before any repository tool call completes. The route is left dead and the error only prints ACPX stderr, which contains `agent starting`; the actionable permission-denied detail is omitted from Boop's diagnostic.

## Reproduction

Environment:

```text
boop source: crates/boop/src/cli/acpx.rs
ACPX package: acpx@0.13.1
ACP agent: opencode
model: openrouter/z-ai/glm-5.3-flash
working directory: a writable Sprefa worktree
```

Commands:

```sh
boop --preset glm53f --name v7-cl-inventory-glm
```

Then send any prompt that reads or writes a file. Boop runs this argument shape:

```text
acpx --format text --ttl 0 --model openrouter/z-ai/glm-5.3-flash opencode -s v7-cl-inventory-glm <prompt>
```

Observed result:

```text
failed (exit status: 5)
[acpx] session ... agent starting
```

`acpx@0.13.1` defines exit status 5 as `PERMISSION_DENIED`. Its non-interactive permission options include `--approve-all`, `--approve-reads`, `--deny-all`, and a JSON policy. Boop's coordinator path passes none of them.

Direct verification with the existing session:

```text
acpx --format text --ttl 0 --approve-all --model openrouter/z-ai/glm-5.3-flash opencode -s v7-cl-inventory-glm <prompt>
```

This connects and exits successfully instead of returning status 5.

## Root Evidence

`crates/boop/src/cli/acpx.rs` contains two duplicated base argument lists, one in `prompt_args` and one in `run_foreground`. Both contain only `--format text --ttl 0` before the optional model and agent.

`checked` captures `std::process::Output`, then reports only `output.stderr` when ACPX exits nonzero. ACPX can put useful structured or text diagnostics on stdout, so the current failure report loses the permission detail.

A local uncommitted hypothesis patch in the primary checkout currently:

1. factors `base_args()` with `--format text --ttl 0 --approve-all`
2. uses it for session creation and prompt delivery
3. adds `output_detail(stdout, stderr)` so both streams survive failures
4. updates the two argument-vector tests
5. adds a deterministic output-detail test

The targeted command passed after a cold build:

```text
cargo test -p boop --bin boop cli::acpx::tests -- --nocapture
3 passed; 0 failed; 69 filtered out
```

Review the permission choice against Boop's coordinator contract. The requested behavior is a non-interactive, writable coordinator used for delegated work inside its selected cwd.

## Acceptance Criteria

- [x] `boop --preset glm53f --name <route>` can read and write inside its designated worktree without ACPX status 5.
- [x] Session ensure and every later prompt use the same explicit ACPX permission policy.
- [x] A nonzero ACPX result preserves meaningful stdout and stderr in Boop's error.
- [x] Argument construction has one source of truth.
- [x] Focused unit tests cover permission arguments and dual-stream diagnostics.
- [x] A live rebuilt-Boop smoke test makes GLM create one proof file inside a temporary or designated worktree path.
- [x] The proof file is inspected and removed after the smoke test.
- [x] The fix is committed with `Refs-Issue: @boop-acpx-permissions`.

## Tests Run

```text
cargo test -p boop --bin boop cli::acpx::tests -- --nocapture
```

The delegating agent must run the focused test after its final edit and one live ACPX smoke. Avoid the full workspace suite unless focused evidence exposes a broader failure.

## Implementation Notes

Work only in the assigned Hafley Rust worktree. Re-find symbols because line numbers drift. Preserve unrelated files and existing dirty chat logs. Review the local hypothesis instead of accepting it mechanically. Install the rebuilt Boop binary only after tests and the live command succeed. Commit the fix and issue update; do not push.

## Resolution

`acpx@0.13.1` defaults to `DEFAULT_PERMISSION_MODE = "approve-reads"` and
`DEFAULT_NON_INTERACTIVE_PERMISSION_POLICY = "deny"` (`dist/cli.js:1398`). No
`~/.acpx/config.json` exists on this machine, so an unflagged write request was
denied non-interactively and acpx exited 5.

Each queued prompt carries its own `permissionMode` to the queue owner
(`dist/output-BZQE0gI1.js:1054`, applied by `updateRuntimeOptions` at
`dist/live-checkpoint-BSIrfgVo.js:4171`), so passing the policy only at session
ensure would not have held for later prompts. `base_args` therefore emits
`--approve-all --non-interactive-permissions deny` on ensure and on every prompt.

`crates/boop/src/cli/acpx.rs`:

- `base_args(model)` is the single source for the global flag vector, including
  the `--model` pair; both `prompt_args` and `run_foreground` call it.
- `output_detail(stdout, stderr)` keeps both streams in the `checked` error.

Live smoke, rebuilt debug binary, temp git worktree, session `acpx-smoke-83019`:

```text
registered acpx-smoke-83019 -> opencode ACPX session
[tool] acpx-proof.txt (completed)
  kind: edit
  input: /private/tmp/boop-acpx-smoke-83019/work/acpx-proof.txt
  output: Wrote file successfully.
[done] end_turn
```

`acpx-proof.txt` read back as `acpx write permission ok.`; the temp tree was removed.

```text
cargo test -p boop --bin boop cli::acpx::tests -- --nocapture
4 passed; 0 failed; 69 filtered out
```
