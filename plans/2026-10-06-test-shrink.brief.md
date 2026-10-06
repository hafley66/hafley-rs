# Brief: shrink the test suite (min test code, max information)

Base: origin/main. Rule source: hafley-rs CLAUDE.md "Tests are min code, max information":
one table-driven test per concern over a fixture directory, whole-output snapshot; a new case is a
fixture file or table row; delete a test whose claim another snapshot already carries.

Today: 1358 `#[test]` (sprefa-extract tests + src, hafley_scm src), 250 files in
crates/sprefa-extract/tests, 58,462 test lines, full suite about 2-4.5 min.

## Phase 1: map (commit 1, no test changes)
`plans/2026-10-06-test-map.md` + ignored TSV under crates/sprefa-extract/bench/test-map/:
per test: file, name, concern (one short noun phrase), command or API it drives, fixture,
assertion kind (snapshot / assert_eq / bool), claim (one line). Use `ryii query` over the test files
to enumerate functions and calls (dogfood); log ryi gaps.
Summary table in the .md: concern, tests, test lines, files, duplicate claims, proposed shape.

## Phase 2: fold, one commit per concern, largest concern first
- Each concern becomes one table-driven test over `tests/fixtures/<concern>/`, one whole-output
  snapshot. Per-case setup lives in fixture files, not code.
- A test is deleted only when its claim appears in a remaining snapshot; the commit message lists
  each deleted test -> the snapshot line(s) that carry its claim.
- No expectation changes. If folding exposes a real difference, stop on that concern and report.
- Shared harness in tests/support/ (one runner); no per-file copies of dispatch/Command boilerplate.

## Gates
- Per commit: the folded concern's tests + t_186 quality gate + the test-module inventory test.
- Final: full suite once on the FINAL commit (after every fix), sprefa-extract
  `--features cli,ts-checker` and hafley_scm `--features rust-checker`; report the counts. The
  coordinator pushes from that report without rerunning.
- Report table at the end: tests_before, tests_after, test_lines_before, test_lines_after,
  suite_seconds_before, suite_seconds_after.

## Limits
KACHE_DISABLED=1. Build/test commands uncapped; other commands under 2 min. No merge, no push.
Do not touch src/ except tests inside src (`#[cfg(test)]` modules), which fold under the same rule.
