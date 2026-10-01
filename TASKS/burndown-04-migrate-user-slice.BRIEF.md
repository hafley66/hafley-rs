# Burndown item 4: migrate-user-slice (issue `migrate-user-slice`)

The user slice is the user's own data: agent_favorite (306 rows), agent_tag (418), agent_tag_link, mood,
dict_mood_name, agent_turn_comment*. Irreplaceable.

## HARD SAFETY
- NEVER open, migrate or write ~/.agent/boop.db. Do not run any boop binary without the contract sandbox guard.
- Rehearse only on a COPY: `cp ~/backups/boop/boop-20261001-162007.db <lane worktree>/scratch/rehearsal.db`
  (gitignore it; it is 2.5 GB, delete it when done). Never write the backups dir.
- The contract suite tripwire (exit 98) means stop and report.

## Change
1. `agent_favorite.source` is a string today: `turn:<session uuid>:<n>` 261, `codex:` 13, `session:` 2,
   `agent_session:` 1, `turn-range:` 1, empty 23, and 1 `turn:` row whose turn is missing. Replace with typed
   columns (e.g. source_kind enum + session/turn/turn_end/codex ref columns as the data needs; read every kind's
   rows on the copy to decide), keep `markdown_id` and the cached body. A schema migration (next user_version)
   converts every existing row; zero rows lost; the orphan keeps its body and gets source_kind marking it missing.
2. dict_mood_name (closed set, 3 rows) becomes an enum column on mood.
3. Rust: crates/boop-store (+ boop CLI `me favorite`, `db favorite`, `tag`) read/write the typed columns. Rules:
   hafley-rs CLAUDE.md (small numbered new files; no growth of >1000-line files: ident.rs, job.rs are huge).
   `CARGO_BUILD_JOBS=4`; one build at a time.
4. boop tsp: `schema/user/` models updated to the new columns (boop2 worktree:
   `git -C /Users/chrishafley/projects/boop2 worktree add /Users/chrishafley/projects/boop2-user -b burndown/user main`).
   Gates: `node tools/0_key_types.mjs` 0, `pnpm build` green (run node_modules from boop2-harmonize if missing: symlink
   is fine for this worktree).

## Proof
- Rehearsal on the copy: before/after counts per table, every favorite's markdown body byte-identical (sha of
  body per favorite_id before = after), source kinds tallied before/after. Put the tallies in the report.
- `cargo test -p boop-store -p boop --bin boop` green; contract suite from boop2-user with BOOP_BIN=<built boop>:
  0 fail, user-slice cases updated if output changed (explain each snapshot diff).
- A migration test (Rust) on a small fixture covering every source kind including empty and orphan.

Commits end with `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`. Do not push; do not
install boop. Report the tallies and every snapshot diff reason.
