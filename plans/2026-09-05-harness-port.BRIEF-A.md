# Lane A: move instant's harness transcript logic into boop-harness

You are pass 1 of 2. Favor plain code. If reality deviates from this brief, STOP and write REPORT.md describing the deviation; do not improvise.

## Plan
Read `/Users/chrishafley/projects/hafley-rs/plans/2026-09-05-harness-logic-into-boop.PLAN.md` first. Section 2 gives the exact type signatures; implement those, no others.

## Sources to move (READ ONLY, absolute paths, do not edit)
- /Users/chrishafley/projects/instant-worktrees/dev/src-tauri/src/0_harness_store.rs
- /Users/chrishafley/projects/instant-worktrees/dev/src-tauri/src/ledger.rs
- /Users/chrishafley/projects/instant-worktrees/dev/src-tauri/src/0_harness_store_tests.rs

## Files you own (inside $PWD, your worktree)
- crates/boop-harness/src/transcript.rs (new)
- crates/boop-harness/src/harness.rs (trait methods only, add at the end of `trait Harness`)
- crates/boop-harness/src/harness/claude.rs, codex.rs, kimi.rs, opencode.rs (impl blocks)
- crates/boop-harness/src/registry.rs (4 helper fns)
- crates/boop-harness/src/live.rs (`session_in_pane` + its two private helpers)
- crates/boop-harness/src/lib.rs (`pub mod transcript; pub use transcript::{Message, SessionMeta};`)
- crates/boop-harness/Cargo.toml (only if a dep is missing; rusqlite and serde_json are already there, check before adding)
- crates/boop/src/chat.rs: delete. crates/boop/src/lib.rs: remove `pub mod chat;`. If anything else references `chat::`, STOP and report.
- crates/boop-harness/src/transcript_tests.rs (new): every `#[test]` from 0_harness_store_tests.rs and every `#[test]` inside ledger.rs, same test names, adapted to the new fn names.

Do not touch any other file. Do not touch the instant repo.

## Rules
1. Move code verbatim. Rename symbols only as the plan names them. No behavior change, no "improvements", no reformatting of moved bodies beyond what rustfmt does.
2. `crate::ledger::iso_to_ms` becomes `crate::transcript::iso_to_ms`. `crate::AiMessage` becomes `crate::transcript::Message`. `HarnessSession` becomes `SessionMeta`. `Editor` enum disappears; `HarnessId` takes its place (it already serializes to the same lowercase tag; verify with a test).
3. instant's `home()` (ledger.rs:20) and root lookups: reuse what boop-harness adapters already use for their roots (`session_roots`, see harness.rs:292). If no equivalent exists for a harness, keep a private `home()` in transcript.rs.
4. `boop_mux_session` tauri wrapper stays in instant; only its body moves into `live::session_in_pane(registry, pane, mail_dir)`. The `INSTANT_TMUX_SOCKET` and `BOOP_MAIL_DIR` env reads stay on the instant side; `session_in_pane` takes the resolved pane id and mail dir as arguments.
5. Wire compatibility test: serialize one `Message` and one `SessionMeta` with serde_json and assert the exact key set listed in plan section 4.
6. Banned identifiers: provenance, substrate, load-bearing, regime. Do not introduce them.
7. Comments: keep the moved doc comments. Drop the two `// boop-harness gap:` comments since the gap is now filled.
8. PRINCIPAL-TRAIT LAW (user, 2026-09-05): every harness-specific body lives inside that harness's adapter file as a method or private fn of the `impl Harness for X` module: `read_claude`, `claude_text`, `classify_user_line`, `injected_tag`, `diagram_write_fence`, `content_has_tool_result`, `first_text` -> claude.rs; `read_codex`, `codex_value_text`, `tool_result_text` -> codex.rs; `read_kimi`, `kimi_wire_meta`, `wire_num`, `kimi_state_path` -> kimi.rs; `read_opencode`, `opencode_message_text` -> opencode.rs. `transcript.rs` keeps only harness-neutral items: the two types, `iso_to_ms`, `chrono_lite`, `mtime`, `created`, `json`, `head_values`, `tail_values`, `usage`, `cap`, `preview_of`, `Extracted`. The kimi main-agent filter in `Registry::sessions_in_cwd` becomes a trait method `fn lists_session(&self, session: &SessionRef) -> bool { true }` that kimi overrides with `session.parent.is_none()`. Acceptance: `grep -n "HarnessId::" crates/boop-harness/src/transcript.rs crates/boop-harness/src/registry.rs crates/boop-harness/src/live.rs` prints nothing, and no `match session.harness` outside adapter files.

## Validation (run all, paste output into REPORT.md)
```
export CARGO_TARGET_DIR=$HOME/.cache/cargo-target/harness-port
cargo test -p boop-harness 2>&1 | tail -30
cargo build -p boop 2>&1 | tail -5
cargo clippy -p boop-harness --all-targets -- -D warnings 2>&1 | tail -20
```
All three must be green. If `cargo build -p boop` fails because something else used `chat::`, STOP and report.

## Commit
One commit on your branch, subject exactly:
`boop-harness: describe, messages, session_by_id, session_in_pane move in from instant; chat.rs retired`

## REPORT.md (worktree root)
- table: instant fn -> boop fn, one row per moved fn
- test names ported (count) and the three validation outputs
- anything you could not move verbatim, with the reason
