# Brief: scm++ ancestor walk must terminate on equal-span nodes

Base: origin/main. Evidence (commit 30722bd8 message): the repository quality gate's scm++ query walks
CST ancestors with a recursive UNION ALL keyed by spans. In rust_rename.rs an if_expression and its
expression_statement share span 7753..10036, so the walk cycles forever (gate stuck > 10 min). 30722bd8
only moved the helper so this file no longer has the shape; the engine bug remains.

## Work, failing test first
1. Fixture: smallest Rust (and TS) source where two nested CST nodes share a span, plus a has-ancestor
   scm++ rule over it. Test runs `ryii query` with a 10 s timeout and snapshots the rows.
2. Fix in the scm++ compiler (generic, every language): ancestor recursion keys on node identity
   (node id / preorder index), not span; or UNION (set) with a visited key. Use the existing
   ancestor-pair store if present (scmpp_ancestor); one implementation.
3. Revert nothing from 30722bd8 (the helper move is fine as code), but confirm the quality gate passes on
   the pre-30722bd8 shape via the fixture.
4. Measure the existing has-ancestor stress cases (plans/2026-10-02-ryi-graph-scmpp-stress.md) before/after,
   single runs.
Full suite once on the final commit; coordinator pushes from it. No merge/push.
