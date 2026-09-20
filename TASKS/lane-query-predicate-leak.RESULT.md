# Lane result: fix the cross-pattern predicate leak in `ryi query`

Boop-Status: done

## Outcome

`matches_predicates` in `crates/sprefa-extract/src/lang/2_source_query.rs` gated
every match by every OTHER pattern's predicates. A pattern with no predicates of
its own inherited foreign `#eq?` checks, failed the capture-length comparison
inside `predicate_matches`, and its matches were dropped silently. The second
loop is deleted. A match is now gated by its own pattern's predicates only; the
empty slice folds to `true`. No flag, no compatibility path.

Both row paths route through the fixed function: `query_tree_sitter` projects
from `query_tree_sitter_spans`, which calls `collect_spanned_matches`, which
calls `matches_predicates`.

## Row counts, before and after

The lane's measured reproduction, two patterns with one `#eq?` on the second:

- before the fix: 1 row (`{"end_line":2,"line":2,"var":"needle"}`); `outer` and
  `other` were dropped
- after the fix: 3 rows, `fn`=`outer` (line 1), `var`=`needle` (line 2),
  `fn`=`other` (line 6)

Full gate from `crates/sprefa-extract`, per-binary `test result:` lines summed:

- baseline immediately before this lane: 186 binaries, 1007 passed, 0 failed
- after this lane: 187 binaries, 1012 passed, 0 failed
- delta: +1 binary (`145_query_predicate_scope`), +5 passed (the new tests),
  0 failed

`cargo metadata --locked --format-version 1` -> `LOCK_OK`.

## New tests, `crates/sprefa-extract/tests/145_query_predicate_scope.rs`

1. `a_predicate_on_one_pattern_never_gates_another_patterns_matches` - the leak
   itself; failed before the fix with left `[(2, "var", "needle")]` against the
   3-row literal
2. `a_patterns_own_predicate_still_filters_its_matches` - 0 matches; predicates
   still filter their own pattern
3. `two_predicates_reach_only_their_own_patterns_matches` - 2 matches; each
   pattern's `#eq?` satisfied by its own captures
4. `a_failing_predicate_drops_only_its_own_patterns_matches` - only pattern A's
   2 matches survive a failing `#eq?` on pattern B; failed before the fix with
   `[]` against the 2-row literal
5. `a_single_pattern_with_a_holding_predicate_is_unchanged` - 1 match; the
   common path

Phase 1 receipt: the leak case failed on unmodified source,
`left: [(2, "var", "needle")]`, `right: [(1, "fn", "outer"), (2, "var",
"needle"), (6, "fn", "other")]`, `3 passed; 2 failed`.

## Deleted, exact

```diff
-    let direct = query.general_predicates(found.pattern_index);
-    if !direct.is_empty() {
-        return direct.iter().try_fold(true, |matched, predicate| {
+    query
+        .general_predicates(found.pattern_index)
+        .iter()
+        .try_fold(true, |matched, predicate| {
             Ok(matched && predicate_matches(predicate, found, source)?)
-        });
-    }
-    for pattern in 0..query.pattern_count() {
-        if pattern != found.pattern_index {
-            for predicate in query.general_predicates(pattern) {
-                if !predicate_matches(predicate, found, source)? {
-                    return Ok(false);
-                }
-            }
-        }
-    }
-    Ok(true)
+        })
```

Net: 5 insertions, 15 deletions in `matches_predicates`. Nothing else changed.

## Scope held

Untouched: `predicate_matches`, `rewrite_predicates`, `validate_predicates`,
`query_language`, `5_scm_lower.rs`, `Cargo.toml`, `Cargo.lock`. No new
dependency, no CLI verb change, no new predicate operators.
