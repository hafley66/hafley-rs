# Boop lifecycle consolidation and live Codex proof

## User mandate

Fresh Astra, maximum effort, through a joinable Boop lane. Review Boop thoroughly with `boop --help`, `extract --help`, `sem --help`, source inspection, grep and rg. Enumerate every feature, remove redundant implementations and ambiguous ways of performing the same operation, and contain harness-specific behavior within the existing harness trait/adapters. Implement fixes, not a report-only audit. Prove the real Codex lifecycle through the shell wrapper, including resume, model/effort changes, compact/clear, and resume again. Keep committing. The user will join this lane; do not close it merely because a report exists.

Work ONLY in the worktree created by Boop for `refactor/boop-lifecycle-consolidation`, based on the commit containing this brief. Main checkout and other lanes are user-owned. Read applicable AGENTS.md and skill instructions first. Re-find symbols; line numbers below are evidence pointers, not stable APIs. No sprefa compiler/kernel or game changes.

## Concrete incident to reproduce first

Parent route `sprefa-ivm-extract-parent`, actual Codex thread `01a067ea-5289-7092-9d52-3588c1af9555`, source cwd `/Users/chrishafley/projects/sprefa`, pane `%384`, tmux session `projects-4`.

1. `boop beep agent register NAME --kind coordinator --harness codex --cwd ...` creates a route with session_id=None. Native workers sent messages; database retained them, delivery said `held-for-turn-boundary (no live codex session ...)`.
2. Installed `boop adopt` is unrecognized although source documentation mentions it.
3. `boop beep lane patch NAME --tmux %384 ... --session-id THREAD` returns exit 0 while printing refusal: no such tmux session %384.
4. Using `--tmux projects-4` reports adopted and attaches the correct thread, but changes the route kind to lane. There is no lane supervisor for the existing parent TUI. Later messages are held for `lane supervisor` and never pushed.
5. Source `cli/job.rs::run_agent` writes session_id=None on registration. `cli/me.rs::run_adopt_with` accepts a kind from caller. `boop-proc/src/deliver.rs` short-circuits kind=lane before attempting a harness door. Confirm current code independently.

Recorded messages include m-5e01c04b, m-82efce39, m-c859fbfb, m-67c67b71. Parent saw native completions but no Boop push. Mailbox insertion, queue admission, and a sent/ack flag are insufficient proof of receipt in the intended live Codex transcript.

Do not mutate or kill that live parent, its panes, unrelated agents, production mail, credentials, favorites, or transcript store as test fixtures. Reproduce with isolated test-owned routes and sessions. A narrowly scoped repair to the task's own parent route may be proposed with evidence. Do not require the human to poll boop wait to make push work.

## Inventory and consolidation

- Enumerate public CLI commands, aliases, hidden commands, generated shell functions, config/presets, agent/lane lifecycle, routing/delivery, parent/child identity, model/effort propagation, transcript ingestion/query/favorites/tags, issue-facing integrations, telemetry, persistence and failure handling. Trace each from help to code to tests. Capture omissions and contradictory help.
- Inventory all harness name/enum matches and string comparisons across the workspace with file, symbol, count and role. Move behavioral dispatch into the existing harness abstraction/adapters. Static serialization/schema data and tests must be distinguished and justified rather than blindly deleted. Add an enforceable architectural guard against new behavioral matches outside that boundary.
- Trace process/thread/session/trace/route/pane identities over time. Assign one owner and update path per datum. Eliminate redundant dispatch, synchronization, discovery, wrappers, fallbacks and identity stores only with migrated consumers and preserved feature behavior. Do not add another generic router/coordinator framework beside the current one.
- For each redundancy, show concrete competing paths, canonical implementation, migrated callers/tests, removal and remaining compatibility obligations. Reuse existing libraries; no subjective build-vs-buy claims. Do not erase supported features to lower counts.
- Enumerate all supported harness adapters. Exercise their existing deterministic contracts. Live Codex is mandatory; mark unavailable live harness environments explicitly.

## Live executable acceptance

Build and run the worktree Boop binary. Test the actual generated `eval "$(boop shell-init bash)"` wrapper and the user's relevant supported shell variant, not a hand-built command that bypasses it. Start a real installed Codex TUI in a test-owned PTY/tmux session using existing authenticated tooling without copying/printing credentials. Read local Codex help/source for actual supported controls. The audit worker itself uses gpt-6-astra max; bounded live child prompts can use an available economical model as long as real model changes are tested and explicitly recorded.

For each transition, capture command, executable build identity, actual model and reasoning effort, route kind, parent, pane/PID, real thread/session/trace IDs, delivery transitions, and transcript evidence:

1. Fresh wrapped launch. Human can attach and interact. Correct identity is registered automatically.
2. Incoming unique nonce from another route is received and answered by that live Codex without a receiver-side boop wait, DB poll, fabricated transcript or manual paste.
3. Message while busy, then while idle, exactly once and to the correct session. Duplicate/retry behavior is explicit.
4. Exit and explicit resume of the same conversation, including another process. Route rebinds to observed live state, preserves intended history/parent linkage and accepts another nonce.
5. Supported model change and effort change within a session and across resume. Verify live transcript/metadata, not just config or launch arguments. No silent fallbacks.
6. Actual compact and clear/new-session transitions as supported by the installed Codex. Record which identity stays or changes; old routes never target a wrong/stale thread. Repeat nonce delivery and resume after each applicable transition. An unsupported command is an explicit unverified case, never a fake pass.
7. Abnormal exit, stale route, supervisor restart/reattach, child completion and parent notification, separate concurrent sessions in the same cwd, clean test-only cleanup.

Use strict bounded timeouts and test-owned process-group cleanup. Do not burn prompts to force compaction when an explicit supported control exists. Request clarification if the only method requires substantial usage beyond these bounded lifecycle trials. A blocked live case stays open with the exact error and next requirement; mocks do not replace it.

## Tests, receipts and delivery

Reuse existing lifecycle, adapter, wrapper, delivery and integration harnesses first. Run failing reproductions before fixes and rerun after. Add deterministic regression coverage plus separately labeled opt-in authenticated live E2E coverage. Keep tests isolated from the user's production DB/config/lane registry; use existing injection points and scratch roots. Give this lane a dedicated CARGO_TARGET_DIR. Do not overwrite HOME/CODEX_HOME or credentials. No broad recursive deletion, no registry rebuild, no main branch merge/push or global boop install without user approval. Use apply_patch for edits.

Establish relevant cargo package gates from manifests; run focused suites after each chunk and the complete affected-package build/tests before handoff. Avoid repeatedly running unrelated giant workspace suites. Report exact current results, skips and environment blockers. Commit each coherent cluster using repository instructions; preserve unrelated diffs. No automatic merge. Save raw live receipts in task-owned storage, redact secrets, and commit concise evidence/manifests with hashes or paths. Do not commit complete user transcripts.

Required artifacts, numbered in reading order under `crates/boop/reports/lifecycle-consolidation/`:

- `0_feature_inventory.md`: complete observed feature/call-path/test matrix and canonical paths.
- `1_harness_boundaries.md`: behavioral match inventory and before/after consolidation evidence.
- `2_live_codex_receipts.md`: tested lifecycle matrix with precise actual PASS/FAIL/BLOCKED results and transcript receipts.
- `3_handoff.md`: commits, remaining issues, test commands, filesystem map, exact join/resume instructions.

Persist this task state before context compaction. Continue through executable fixes and live proof while authorized work remains. Report milestones with `boop beep parent '<concise evidence>' --no-wait`; if that path fails, record the actual delivery failure in handoff and stay joinable. Completion means tested implementation plus an honest remaining-gap ledger, not a claim of eternal maintenance freedom.
