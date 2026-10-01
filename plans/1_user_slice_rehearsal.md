# migrate-user-slice: version 39

Rehearsal used only a copy of `~/backups/boop/boop-20261001-162007.db` in this lane's `scratch/rehearsal.db`. The reproducible proof is [0_user_slice_rehearsal.sh](0_user_slice_rehearsal.sh), using bash, sqlite3, xxd, and shasum. Per-favorite body hashes are [2_user_slice_body_sha256.tsv](2_user_slice_body_sha256.tsv).

The measured backup inventory differs from the original brief. Version changed from 38 to 39. All 303 favorite IDs, markdown IDs, notes, original source strings, creation timestamps, and body SHA-256 hashes matched. Complete tag and comment SQL dumps matched; mood IDs, names, and template bytes matched. No missing `turn:` target was present in this backup. The small Rust migration fixture covers a missing turn and retains its body and reference under `missing_turn`.

| Table | Before | After |
|---|---:|---:|
| agent_favorite | 303 | 303 |
| agent_tag | 418 | 418 |
| agent_tag_link | 841 | 841 |
| mood | 3 | 3 |
| dict_mood_name | 3 | removed |
| agent_turn_comment | 234 | 234 |
| agent_turn_comment_target | 210 | 210 |
| agent_turn_comment_fork | 9 | 9 |

Before migration, source categories were counted by their exact colon prefix, with unstructured values under `other`. After migration, categories were counted from the enum column.

| Before category | Before count | After kind | After count |
|---|---:|---|---:|
| turn | 263 | turn | 263 |
| codex | 13 | codex | 23 |
| session | 2 | session | 9 |
| agent_session | 1 | agent_session | 1 |
| turn-range | 1 | turn_range | 1 |
| empty | 1 | empty | 1 |
| other | 22 | text | 5 |

Ten prose Codex references join `codex`; seven prose/bare session references join `session`; five unstructured values remain `text`. `source_text` preserves every original value. The three legacy `session:3009` / `agent_session:3009` references resolve dictionary row 3009 to stable session text `019ffb9b-51cb-7e92-be44-4eb469f46d95`. Rebuild carries typed references directly, preserving missing markers and edited mood templates.

Contract fixtures now contain version 39 user tables and the three mood seeds. Setup performs no boop invocation before deterministic fixtures allocate IDs, removing the `dict_session.id` collision. The runner's favorite/tag COUNT tripwire is restored; its unguarded `--version` invocation remains removed. Only the runner's explicitly authorized read-only COUNT query opens the live database.

Every changed snapshot is under `tests/snapshots/1a_user.bats/` in boop2-user:

| Snapshot filename | Diff reason |
|---|---|
| favorite_add_reads_file_and_stdin_and_shares_cached_markdown.snap | File and stdin sources use `text`; SQLite rows include the typed reference columns and original text. |
| favorite_delete_keeps_cached_body_and_missing_id_fails.snap | The surviving favorite has `session` kind and parsed session `second`. |
| favorite_edit_preserves_body_and_omitted_metadata.snap | Editing to `url:changed` sets `text` and clears former session reference fields; the second favorite retains its parsed session. |
| favorite_list_limits_newest_first_and_bare_group_requires_a_verb.snap | NDJSON exposes typed reference fields; SQLite rows store kind/session separately. |
| favorite_show_includes_body_and_missing_id_fails.snap | NDJSON exposes typed reference fields; SQLite rows store kind/session separately. |
| me_favorite_selects_newest_and_older_assistant_turns_with_note_tags.snap | SQLite rows store `turn`, conversation, turn index, harness, and assistant role separately. |
| me_mood_sets_reads_clears_and_inherits_parent_mood.snap | Unknown-mood diagnostics include `plain`, which the version 39 fixture now seeds alongside board and unga. |
| tag_backfill_applies_favorite_notes_once.snap | Favorite SQLite rows include typed session references; tag rows and output are unchanged. |
| tag_search_matches_tag_substring_and_ignores_favorite_body.snap | Favorite SQLite rows include typed session references; search results and tag rows are unchanged. |

Rust verification: 239 store unit tests, 2 new user-slice integration tests, and 2 existing concurrency integration tests passed. The requested `cargo test -p boop-store -p boop --bin boop` passed 170 tests. The fixed August RSS fixture's test window now starts at epoch zero so its row remains included on October 1. Production RSS behavior is unchanged.

TypeSpec: `node tools/0_key_types.mjs` exited 0 with `multi_type_keys=0`; `pnpm build` compiled successfully. `mood.name` documents its closed-enum exception to the shared `name` key check.

Final full contract run: exit 0, 75 cases, 71 passed, 4 existing v1 capability skips, 0 `not ok`. Skips cover external target-root enforcement, the absent gc verb, ACP lanes without tmux panes, and config-only harness registration. The favorite/tag COUNT tripwire passed. No mail or lane snapshots changed.

Cleanup: `scratch/rehearsal.db` (2.5 GB), `scratch/edit_user.py`, `scratch/proof.py`, and their temporary JSON proof files were deleted. The committed proof uses sqlite3/bash. No boop installation or push was performed.

The matching boop2-user commit is `f56c441` on `burndown/user`.
