# Brief: ryi / ryii help is progressively disclosed and drives the next command

Base: origin/main. CLAUDE.md rules apply (generic first, one implementation per concern, tests as
fixture tables). Rust workspace profile rule: `[profile.dev.package."*"] opt-level = 3`,
`[profile.dev] opt-level = 1`.

## Reference and anti-reference (run both before writing anything)
- Reference: `bew --help` (41 lines, 2.8 KB). Source: ~/projects/hafley-rxjs/packages/bewpp/src/cli/
  (7_dispatch.ts help text, 2_budget.ts, 3_regions.ts + 4_handleStore.ts + 6_expand.ts handles,
  6_renderGeneric.ts `next:` footer). Read those files.
- Anti-reference: `boop --help` (477 lines, 30 KB). Every property it has is a defect here.

What bew does, as rules for ryi:
1. Top-level help = one line per verb: `ryi <verb> <arg shape>   what it returns / when to use it`.
   Verbs listed in workflow order (find -> ask -> edit), not alphabetical, not clap's default dump.
2. No flag list at top level. Flags live in `ryi <verb> --help`, and only flags of that verb.
3. Top-level help contains only one line per verb. Do not add an invariants block anywhere.
4. Every human-rendered output ends with a `next:` footer of 1-3 runnable commands filled with real
   values from that output (HATEOAS). `--no-next` hides it. `--json`/JSONL stays raw and unclipped.
5. Output to a terminal/agent has a byte budget; withheld rows collapse into a handle the user can
   expand (`ryi x <handle>`) or filter (`ryi grep <handle> <text>`), as bew does. If ryi already has a
   row budget or trail ids, reuse them; one implementation.
6. Errors name the exact command that fixes them (bew: "Restart it with: bew bridge restart").
7. Bare `ryi` with no paths already suggests next steps (src/bin/ryi/1c_no_inputs.rs): fold it into
   the same `next:` renderer, not a second one.

## Surfaces
- `ryii` (crates/sprefa-extract/src/bin/ryi.rs, src/bin/ryi/0_cli.rs, clap) and the `ryi` client
  (crates/ryi). Same text for both; one source. Keep clap; use its help_template / per-subcommand
  override, not a hand-rolled parser.
- Today: top-level 56 lines with 23 flags; per-verb 6-29 lines; installed binaries in ~/.cargo/bin
  are from 2026-09-27.

## Work, one commit each
0. Snapshot today's help: one fixture-table test over every `--help` (top level + each verb) and
   the bare `ryi` output, whole-output snapshot. This is the before.
1. Top-level help rewritten per rules 1-3. Target <= 45 lines, <= 3 KB.
2. Per-verb help: usage line, 2-4 real example invocations against this repo, the verb's flags
   grouped (common first). Target <= 25 lines each. Global flags appear once under `ryi help flags`.
3. `next:` footer for graph, rename/move/cleave dry-run, query, capabilities, bare `ryi`.
4. Budget + handles if not already present (rule 5).
5. Every example in help text is executed by the snapshot test against a fixture or this repo and
   its exit code is 0 (no stale examples).

## Gates
Per commit: help snapshot test + t_186 + module inventory. Final: full suite once on the final
commit (sprefa-extract `--features cli,ts-checker`; hafley_scm `--features rust-checker`), counts in
the report. Report: before/after lines and bytes for top-level and each verb help. No merge, no push.
KACHE_DISABLED=1.

## Added 2026-10-07: stale-binary warning (user-set, do this commit FIRST)
User saw `ryi`/`ryii` in ~/.cargo/bin built 2026-09-27 while main had moved weeks ahead. Never again.
- Build time (existing build.rs that prints "Build: git hash"): embed the commit hash AND the
  absolute repo root the binary was built from.
- Run time, opt-in via env `RYI_STALE_CHECK=1` (the user sets it in the shell profile): if the
  embedded repo root exists on this machine, read its `main` ref directly (`.git/refs/heads/main`,
  else `packed-refs`; for a worktree follow `.git` -> commondir; no git subprocess). If it differs
  from the embedded hash, print ONE stderr line, then continue normally:
  `ryi: stale build <hash> (<date>) from <root>; main is <hash>. Rebuild: <exact install command>`
  Repo root missing, ref unreadable, or env unset: print nothing.
- Same check in both `ryi` (crates/ryi) and `ryii`; one implementation, shared crate.
- `ryi --version` prints hash, build date, repo root, and the stale verdict regardless of the env.
- Test: fixture table over a temp dir with a fake `.git` (loose ref, packed-refs, worktree
  commondir, missing root, env unset) -> expected stderr line or empty. Whole-output snapshot.
- The rebuild command printed is `cargo install --locked ...` (without --locked, cargo install
  resolves fresh deps and ra-ap-rustc_lexer fails: unicode-properties vs unicode-ident version
  mismatch, seen 2026-10-07).
