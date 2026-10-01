# Burndown item 5: dict-closed-sets (issue `dict-closed-sets`)

Turn the closed-set `dict_*` interning tables into enum columns (dict_mood_name is already done in schema 39).
Closed sets (live rows): dict_attach 5, dict_attr_key 5, dict_edekind 9, dict_harness 6, dict_netkind 2,
dict_observation_source 3, dict_price_source 2, dict_role 6, dict_session_relation_kind 0, dict_status 3,
dict_trace_classification, dict_trace_delivery, dict_trace_kind, dict_verb (check counts). Each referencing
`*_id INTEGER` column becomes a `TEXT` column with `CHECK (col IN (...))` (values from the boop tsp enums),
the dict table is dropped, in one migration to schema 40. Keep open dicts (dict_record, dict_request, dict_path,
dict_session, ...) unchanged. If a "closed" table turns out to grow with data, leave it and say so.

## HARD SAFETY (same as item 4)
Never open or write ~/.agent/boop.db; rehearse only on a copy of ~/backups/boop/boop-20261001-162007.db in the
lane's scratch/ (delete after); contract tripwire exit 98 = stop. No Python (sqlite3/bash only for proof).

## Proof
Rehearsal on the copy (38 -> 39 -> 40): per affected table, row count before = after, and the decoded value
multiset equal (e.g. `SELECT r.value, count(*) FROM agent_turn t JOIN dict_role r ... GROUP BY 1` before vs
`SELECT role, count(*) FROM agent_turn GROUP BY 1` after). Query timings for `boop db search` and one
agent_turn read before/after on the copy. Proof scripts and TSVs committed under plans/.

## Code and tsp
Rust: crates/boop-store (+ callers). CLAUDE.md rules (ident.rs and job.rs are huge: new code in new numbered
files). CARGO_BUILD_JOBS=4. `cargo test -p boop-store -p boop --bin boop`.
boop tsp (boop2 worktree `git -C /Users/chrishafley/projects/boop2 worktree add /Users/chrishafley/projects/boop2-dict -b burndown/dict main`,
symlink node_modules from /Users/chrishafley/projects/boop2-harmonize): table models use the enums; the 0_shared Dict
generic covers only the open dicts. Gates: `node tools/0_key_types.mjs` 0, `pnpm build` green, contract suite
0 not-ok with BOOP_BIN=<built boop>.
Commit both repos (messages end `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`) BEFORE you
report done. Do not push; do not install boop.
