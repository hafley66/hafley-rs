# Lane B: instant stops owning harness logic; it calls boop-harness

You are pass 1 of 2. Favor plain code. If reality deviates from this brief, STOP and write REPORT.md describing the deviation; do not improvise.

## Context
Lane A landed on hafley-rs main (ca2a7d8). boop-harness now owns everything instant's `0_harness_store.rs` and `ledger.rs` used to do. Read the report first: `/Users/chrishafley/projects/hafley-rs/plans/2026-09-05-harness-port.REPORT-A.md` (table of instant fn -> boop fn). Read `/Users/chrishafley/projects/hafley-rs/plans/2026-09-05-harness-logic-into-boop.PLAN.md` section 2 for the signatures.

The API you call (crate `boop_harness`, already a path dep in src-tauri/Cargo.toml):
- `boop_harness::transcript::{Message, SessionMeta, iso_to_ms}`
- `boop_harness::Registry::{discover, sessions_in_cwd, describe_all, session_ids_for_cwd, messages_by_id}`
- trait `boop_harness::Harness::{describe, messages, resume_id, session_by_id, lists_session}`
- `boop_harness::live::session_in_pane(&registry, pane, mail_dir) -> anyhow::Result<Option<String>>`
- `boop_harness::HarnessId` serializes to the same lowercase tag the old `Editor` enum did.

## Files you own (inside $PWD = /Users/chrishafley/projects/instant-worktrees/harness-out)
- src-tauri/src/0_harness_store.rs: delete every moved fn. Keep ONLY the `#[tauri::command] boop_mux_session`, whose body becomes: resolve `socket` (existing env fallback), resolve `pane` (existing `pane_of_target` / `Tmux.pane_id` chain), resolve `mail_dir` (existing `BOOP_MAIL_DIR` / `default_mail_dir` chain), then `boop_harness::live::session_in_pane(&Registry::discover(), &pane, &mail_dir).map_err(|e| e.to_string())`. Drop `HarnessSession`, `refs`, `sessions`, `session_ids`, `resolve`, `messages`, the four `*_shape`, `interactive_session_id`, `resolve_registered_session`, `route_session_in_pane`, `claude_project_dir`, `claude_session_path`, `resume_id`, `mtime`, `created`, `json`, `head_values`, `tail_values`, `usage`, `kimi_*`, `wire_num`.
- src-tauri/src/0_harness_store_tests.rs: delete the file and its `#[path]` mod line. Its tests now live in boop-harness.
- src-tauri/src/ledger.rs: delete `Editor`, `AiMessage`, `read_claude`, `read_codex`, `read_kimi`, `read_opencode`, `iso_to_ms`, `chrono_lite`, `codex_session_path`, `kimi_session_path`, `home`, every text helper, and every `#[test]`. Keep `AiSession` and the three `#[tauri::command]` fns. `read_ai_messages_blocking` becomes `Registry::discover().messages_by_id(id, &session_id, &cwd, after_seq)`. `list_ai_sessions_blocking` becomes `Registry::discover().describe_all(harness, cwd.as_deref())` mapped to `AiSession` (title fallback: first `role == "user"` message preview via `messages_by_id`, as today). `editor_tag(&Editor)` becomes `editor_tag(id: HarnessId) -> &'static str` returning `id.as_str()` or the existing HarnessId tag fn; check what HarnessId offers before writing a match.
- src-tauri/src/lib.rs: `pub use ledger::AiMessage;` becomes `pub use boop_harness::transcript::Message as AiMessage;`. Keep the invoke_handler list unchanged (same command names).
- src-tauri/src/harness.rs: `crate::harness_store::session_ids(id, &cwd)` becomes `boop_harness::Registry::discover().session_ids_for_cwd(id, &cwd)`. Delete the stale per-harness path comments at the top of the file (they describe logic that no longer lives here); keep the two commands.
- src-tauri/src/favorites.rs: `use crate::ledger::AiMessage;` becomes `use boop_harness::transcript::Message as AiMessage;`. `msg.editor` is now a `HarnessId`; adapt the one call to `editor_tag`.
- src-tauri/Cargo.lock only if cargo rewrites it.

Do not touch any `.ts`/`.tsx` file. Do not touch `ipc/commands.json` or `src/generated/`. The wire JSON is unchanged, so TypeScript stays as is.

## Rules
1. No harness-specific code remains in src-tauri after this lane. Acceptance: `grep -rn "isSidechain\|turn_context\|wire.jsonl\|opencode.db\|\.claude/projects\|\.codex/sessions\|\.kimi-code" src-tauri/src/` prints nothing outside comments in files you do not own. If a hit is in a file you do not own, list it in REPORT.md and leave it.
2. Package manager is pnpm via corepack: `corepack pnpm@10.12.4`. Never run `npm install`.
3. Banned identifiers: provenance, substrate, load-bearing, regime.
4. No behavior change. Same command names, same JSON keys, same ordering.

## Validation (run all, paste output into REPORT.md)
```
export CARGO_TARGET_DIR=$HOME/.cache/cargo-target/instant-harness-out
cd src-tauri && cargo test 2>&1 | tail -15 && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5; cd ..
corepack pnpm@10.12.4 install --frozen-lockfile 2>&1 | tail -2
corepack pnpm@10.12.4 exec tsc --noEmit 2>&1 | tail -5
corepack pnpm@10.12.4 exec vitest run 2>&1 | tail -8
node scripts/generate-native.mjs && git status --short ipc src/generated
```
Expected: cargo test green (90 tests on the base minus the deleted ones), clippy clean, tsc clean, vitest 474 passed, `git status` shows no change under ipc/ or src/generated/. If cargo test count drops by more than the tests you deleted, STOP and report.

## Commit
One commit on your branch, subject exactly:
`refactor(harness): instant calls boop-harness for sessions, messages, and the pane session; harness_store and ledger readers deleted`

## REPORT.md (worktree root)
- lines deleted per file (`git diff --stat`)
- the acceptance grep output
- the validation outputs
- anything you could not delete, with the reason
