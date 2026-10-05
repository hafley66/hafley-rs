# Rules of hooks query fixture

Generic mechanism: join syntax captures to fast call sites and dataflow owners by
input path and byte span, restrict syntax ancestry to the innermost function,
then derive violations in SQL. Framework naming conventions occur only in these
query files. No Rust rule implementation is added.

React source: commit `ae74234eae6ebd62f19190731278e20bc1c37d51` (`v19.2.0`),
[ESLintRulesOfHooks-test.js](https://github.com/facebook/react/blob/ae74234eae6ebd62f19190731278e20bc1c37d51/packages/eslint-plugin-react-hooks/__tests__/ESLintRulesOfHooks-test.js).
The upstream MIT license is included. The complete test declaration file is
preserved as `0_ESLintRulesOfHooks-test.js`.

`node 1_vendor.cjs` evaluates case declarations and upstream message helpers
without loading ESLint or Jest. It verifies the pinned source SHA-256, writes
one exact source file per case, and preserves expected errors and settings in
`1_cases.json`. Both string and object error forms are preserved. Cases retain
their upstream zero-based array index; the parser-version matrix is not duplicated.
There are 50 valid and 68 invalid cases. Four retain `syntax: flow` metadata and
are unavailable to the TypeScript fast adapter.

## Program and adapter

`2_hooks.scm` selects hook calls and requests generic CST ancestor storage.
`3_violations.sql` reads that query database with a fast database attached as
`facts`. Its result signature is:

```text
(path, start, end, hook, owner, rule, status, reason)
```

`status` is `violation` or `gap`. Byte offsets are half-open. Multiple rules may
produce rows at one call. Gap rows mark an attempt unavailable in the adapter.
The authorized pre-ryiii comparison records gaps separately and counts unmatched
expected messages as false negatives.

`4_adapter.cjs` performs one attempt for the future `ryiii` runner:

```sh
node crates/sprefa-extract/bench/rules-of-hooks/4_adapter.cjs \
  crates/sprefa-extract/bench/rules-of-hooks/fixtures/invalid/024_Case.tsx \
  crates/sprefa-extract/bench/rules-of-hooks/runs/invalid-024
```

The default binary is `$CARGO_TARGET_DIR/debug/ryii`, falling back to this crate's
`target/debug/ryii` when the environment variable is unset. An optional third
argument selects another worktree build. The runner supplies a fresh
output directory for each attempt. Each subprocess has a 55-second deadline.
The adapter returns one JSON object with `case_id`, `status`, and `findings`;
unavailable attempts exit 2 and carry a cause or gap findings. It has no iteration
over cases, oracle comparison, or scoring. Registration in `ryiii` is pending
because that runner has no implementation in this checkout.

The two extraction commands used by the adapter are:

```sh
"$CARGO_TARGET_DIR/debug/ryii" --kinds call,df --sqlite FACTS_DB CASE_FILE
"$CARGO_TARGET_DIR/debug/ryii" query --scmpp crates/sprefa-extract/bench/rules-of-hooks/2_hooks.scm \
  --sqlite QUERY_DB CASE_FILE
```

The first is syntax-only extraction. Neither command enables a checker. After
both stores exist, attach `FACTS_DB` as `facts` in `QUERY_DB` and execute
`3_violations.sql`. The adapter performs that attachment with the SQLite CLI.

## Lifetimes and reads

Each attempt creates two stores and exits. Captures join by path: `@invoked` spans join callee-only `site` spans, and full
`@hook` spans join `node(kind=call_res, family=df)` spans. Owner names come from `node.function`; a `::closure::` owner uses the
matching frame's named lambda definition when present. CST preorder intervals choose the innermost function and
exclude conditions, loops, and returns belonging to another function.

JSX elements are excluded from the CST function-frame list. An attribute's
ordinary hook call therefore uses its eager dataflow owner. A deferred JSX
component invocation is not a `call_expression` hook capture. This depends on
the generic deferred JSX lift. Querying the vendored upstream suite exercises
its JSX-bearing cases; the suite does not provide a dedicated JSX owner assertion.

The query uses the terminal identifier of direct or member callees. The SQL
recognizes component and hook owners by `[A-Z]` and `use[A-Z0-9]` prefixes.
Loop rows require matching `df_nest` and `df_loop` facts inside the function.
Conditional rows inspect branch fields and the right operand of `&&`, `||`,
and `??`. Return rows use byte order within the same function.

No file name or validity label participates in violation computation.
`7_expected.sql` imports case metadata and expected messages with SQLite JSON
functions. `8_score.sql` is the single pre-ryiii scoring query: message multiset
matching by `(path, rule, hook)` with unique actual call spans. Source locations
for expected messages are unavailable in the upstream declarations. The
`disagreements` view lists every unequal count with a file and reason.
See `6_REPORT.md` for measured results and remaining limits.

## Reproduce the pre-ryiii comparison

Build from this worktree in `crates/sprefa-extract`:

```sh
KACHE_DISABLED=1 cargo build --features cli,ts-checker --bin ryii
```

Then run from `crates/sprefa-extract/bench/rules-of-hooks` with a fresh `runs`
directory. The compiled checker feature is unused by these extraction commands.

```sh
mkdir -p runs
RYII="${CARGO_TARGET_DIR:-../../target}/debug/ryii"
sqlite3 :memory: "SELECT json_extract(value, '$.file') FROM json_each(CAST(readfile('1_cases.json') AS TEXT)) WHERE json_extract(value, '$.syntax') IS NULL OR json_extract(value, '$.syntax') <> 'flow';" > runs/nonflow.paths
"$RYII" --kinds call,df --sqlite runs/facts.db - < runs/nonflow.paths
"$RYII" query --scmpp 2_hooks.scm --timeout 90 --sqlite runs/query.db - < runs/nonflow.paths
awk 'BEGIN {print "CREATE TABLE actual AS"} {print}' 3_violations.sql > runs/materialize_actual.sql
sqlite3 runs/query.db <<'SQL'
ATTACH 'runs/facts.db' AS facts;
.read runs/materialize_actual.sql
.read 7_expected.sql
SQL
sqlite3 -json runs/query.db < 8_score.sql > 9_SCORE.json
sqlite3 -json runs/query.db 'SELECT * FROM disagreements ORDER BY path, rule, hook;' > 10_DISAGREEMENTS.json
python3 ../../../../scripts/bench_grid.py --json 10_DISAGREEMENTS.json --out 11_DISAGREEMENTS.html --title 'Rules of hooks disagreements: pre-ryiii'
```

The database stores all 118 case metadata rows; expected-message computation and
the extraction path list exclude the four Flow cases. `runs` is ignored by Git.
Result JSON and the grid are committed for review. No ESLint execution, fixture
rewriting, or separate scoring runner participates in the comparison.
