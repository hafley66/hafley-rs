# Pattern-line union: five versions of a design that never shipped

Research retention, 2026-09-18. No action taken. Recovered from
`sprefa-archive-20260428` (v0-v2), `sprefa-archive-20260701` (v3/v4),
`~/projects/sprefa` (v5-v8), and `hafley-rs/crates/sprefa-extract` (today).

The ask that triggered the search: make the OR part of ast-grep's syntax easier.

## Contents

- [One-paragraph state](#one-paragraph-state)
- [The diagnosis, written 2026-04-10](#the-diagnosis-written-2026-04-10)
- [Four spellings of the same idea](#four-spellings-of-the-same-idea)
- [The arc, version by version](#the-arc-version-by-version)
- [What exists in sprefa-extract today](#what-exists-in-sprefa-extract-today)
- [The two laws every version agreed on](#the-two-laws-every-version-agreed-on)
- [Blockers a future attempt inherits](#blockers-a-future-attempt-inherits)
- [Archive map](#archive-map)
- [Recovery commands](#recovery-commands)

## One-paragraph state

`AstRule::Any` is implemented in `crates/sprefa-extract/src/lang/1_ast_rule.rs:27`
and decoded at `:246`. `src/bin/extract.rs` mentions `ast_rule` zero times. The
union is built and has no door. Every custom alternation surface designed
between v0 and v7 is unbuilt.

## The diagnosis, written 2026-04-10

`sprefa-archive-20260428/memory/project_metavar_grammar.md:296`:

> ast-grep has `any:` in rule DSL only | no pattern-line union; `${a,b,c}`
> lowering to `any:` is sprefa's way to surface it in pattern syntax

The policy set in the same memo, `:271`:

> pattern-native first, rule DSL excursion only when forced | stay in pattern
> mode; `${a,b,c}` is the sanctioned union, `:{k|k}` is the sanctioned kind filter

Why the flat pattern grammar was insufficient, `:9`:

> The flat ast-grep pattern syntax was insufficient for extracting enum variant
> names uniformly across unit/tuple/struct variant shapes. Discussion covered
> why ast-grep patterns are 1-to-1 (CST-anchored, tree-sitter emits different
> kinds for variant shapes)

Two other complaints from the same era:

| complaint | file:line |
| --- | --- |
| "ast-grep does exact structural matching on JSON -- can't do partial object match, so custom destructuring engine needed" | `chat_log/20260331.2.sprf-lang-grammar-design.md:49` |
| "CLI command extraction blocked because ast-grep patterns are 1-to-1 and can't iterate enum variants without kind+inside rule objects" | `chat_log/20260409.11.md-tag-ast-fix-selfcheck.md:12` |
| "ast-grep's pattern language is limited, and not everything is AST -- configs, lockfiles, deploy manifests are trees too" | `memory/feedback_sprf_dsl_intent.md:9` |

## Four spellings of the same idea

| era | spelling | scope | lowers to | built |
| --- | --- | --- | --- | --- |
| v0/v1 | `(a\|b\|c)` | every selector level: repo, branch, file, JSON key | selector engine, pre-ast-grep | no |
| v1 | `{a,b,c}` | URTSL path segment | selector engine | no |
| v2 | `${a,b,c}` | inside a pattern, brace expansion at lower time | cartesian expand to N patterns, emit `any:` | **no** |
| v2 | `{ > A ; B }` | rule tree, `;` at sibling level | ast-grep `any:` | no |
| v2 | `${re(A\|B\|C)}` | inside one metavar slot | per-metavar `constraints` | no |
| v6/v7 | `[ (a) (b) ]` | tree-sitter s-expression, native in dl6 | tree-sitter query alternation | no |

Spelling drift inside a single week of v1: `(a|b|c)` in the design chat
(`chat_log/20260330.5:45`), `{a,b,c}` in the spec (`docs/urtsl-spec.md:28`),
then `${a,b,c}` in the final metavar grammar.

### Why `${a,b,c}` won over the alternatives

`project_metavar_grammar.md:32,36`:

> `${ENTITY}` vs `${a,b,c}` | top-level comma inside `${}` → brace expansion;
> else segment capture (existing) | one bracket family, comma presence is the tiebreaker

> `!{a,b,c}` rejected | collides with Rust macro syntax (`vec!{}`, `hashmap!{}`) |
> use `${a,b,c}` which reuses existing `${}` family

### The unshipped implementation step

`project_metavar_grammar.md:277`:

> 1. Lowerer brace expansion pass | scan ast body for `${...}` with top-level
> commas, cartesian-expand into N pattern strings, emit `AstSelector.rule = { any: [...] }`

A second union axis in the same table, `:21`:

> `$$$NAME:{k1|k2}` | fan out filtered to kind union | `$$$_:{enum_variant|struct_item}`

## The arc, version by version

| version | position on alternation | outcome |
| --- | --- | --- |
| v0/v1 | first-class primitive, uniform at every selector level, one of five pattern types | pre-ast-grep, superseded |
| v1→v2 | ast-grep adopted as a backend; the hole gets named | `${a,b,c}` designed in full, never built |
| v2 (alt bet) | invert the policy: surface ast-grep's whole combinator algebra as syntax | `;` = `any`, designed, never built |
| v2 (April 12) | promote OR out of the pattern layer entirely | `{A; B}` was already OR by monomorphization; `path()` adds keys |
| v3/v4 | T2 states the fork bare and picks | relational head union, provisionally adopted |
| v5 | no custom surface at all | `ast_yaml` takes ast-grep YAML verbatim, "supersets sg" |
| v6 | patterns as checked ground terms, not strings | lab measured that the two lowerings disagree |
| v6/v7 | alternation becomes tree-sitter `[ ]`, native in dl6 | planned, not in extract |
| v8 / today | nothing custom | `any:` only through YAML, no CLI flag |

### The v2 pivot: alternation is relational, not syntactic

`chat_log/20260412.5.path-tag-or-branching.md:50`:

> `{;}` is already OR via monomorphization; just lacked keys. Don't invent new
> flow control.

`docs/labeled-branches-path-capture.md:62-69`:

> - `A > B`         — AND (same row, sequential narrowing)
> - `path() {;}`    — OR, keyed, path-tracked (Form B)
> - `path(PAT)`     — filter on accumulated path (Form A)
> - `{ A; B }`      — OR, unkeyed (existing mechanism, `_path` stays NULL)
>
> `mergeByKey` from the rxjs analogy = SQL union on `_path`.

### The v3/v4 decision, already made

`sprefa-archive-20260701/TASKS.md:28-32`:

> - [ ] **T2 (H)** OR / alternation: no `|` in the grammar. Decide a surface:
>         - (a) pattern union at the literal: `q:{ a: $x } | { b: $x }`
>         - (b) datalog-native: multiple `json(...)` rules unioned at the head
>           (works today, no syntax change — probably the right default).
>       Add (a) only if (b) proves too noisy.

`:16` and `:20`:

> `| { a: $x } OR { b: $x } | no | no alternation in the grammar (T2) |`
>
> Workaround for OR today: multiple `json(...)` rules unioned at the head relation.

### The v5 floor

`chat_log/20260630.9:49` says `ast_yaml` supersets `sg`. The instruction given
when an agent hand-rolled a regex heuristic instead:

> lol use the literal ast-grep tools we have rofl

### The v6 rigor: patterns as terms

`git show 502456bc8:v6/prolog/labs/astgrep_patterns.md`, §4 heading:

> ## 4. The two lowerings are not equivalent, and the lab measures the gap

The three measured divergences:

| # | divergence | consequence |
| --- | --- | --- |
| 1 | named `$$$` is inexpressible natively; tree-sitter captures nodes, never sibling lists | emit `blocked(named_ellipsis('REST'))` |
| 2 | child matching is a subsequence by default, so `(arguments (_) @A)` also matches `f(a, b)` | needs the `.` anchor; an empty arg list stays `blocked(empty_child_list_inexpressible)` |
| 3 | non-linearity means two things: term identity vs `#eq?` comparing source TEXT | two different semantics under one spelling |

The design law it produced:

> a two-path lowering owes a written equivalence law and a refusal channel, not
> just two emit functions. The refusal channel is the part that is easy to skip
> and expensive to skip: an emitter that silently drops a named `$$$` produces a
> query that runs, returns matches, and is wrong.

On why a string pipeline cannot self-check:

> a string codemod pipeline can only report zero matches, and zero matches is
> indistinguishable from a broken pattern.

On the quoted-region delimiter choice:

> Primary form is a compile-time quoted region with an explicit closing
> delimiter, `{|sg|| ... |}` shape rather than `sg{ ... }`. Brace balancing
> fails on patterns that legitimately contain unbalanced braces (a JS pattern
> like `function $F() {` is a real ast-grep pattern), and the outer lexer must
> not need a nested parser to find the end.

### The v6/v7 answer: tree-sitter `[ ]` native in dl6

`sprefa/plans/2026-08-04-cst-native-syntax.md:3`:

> User words: "i wanted cst to be native in the syntax... woulda been lispy".

`:15-19`, two alternated node patterns sharing one capture:

```
cst(path, digest, rust) {
  [ (function_item name: (identifier) @function_name)
    (macro_definition name: (identifier) @function_name) ]
  (#match? @function_name "^handle_")
}.
```

`:21-23`:

> The `{ ... }` block is parsed by parse_dl.pl as s-expressions: node kinds,
> field names (`name:`), `[ ]` alternation, `@capture` names, `#match?` /
> `#not-match?` / `#eq?` predicates. NOT a string; a parse error is a dl6 parse
> error with line/column.

## What exists in sprefa-extract today

Crates pinned at 0.38: `ast-grep-core`, `ast-grep-language`, `ast-grep-config`
(`Cargo.toml:44-46`). `Cargo.toml:42` notes these were carried from v0.

### The typed algebra, built

`src/lang/1_ast_rule.rs:27`:

```rust
pub enum AstRule {
    Pattern(String), Kind(String), Regex(String), Matches(String),
    All(Vec<AstRule>), Any(Vec<AstRule>), Not(Box<AstRule>),
    Inside { rule, stop_by }, Has { rule, stop_by },
    Follows { rule, stop_by }, Precedes { rule, stop_by },
}
```

| surface | reachable how | CLI |
| --- | --- | --- |
| `AstRule::Any` | `decode_ast_rule_yaml` (`1_ast_rule.rs:192`) | **none** |
| `--ast-pattern ID=PATTERN` | `extract.rs:235-260` | one pattern, one file, no union |
| `--ast-selector ID=KIND` | requires `ast_pattern`, gives `Pattern::contextual` | no union |
| `--ast-capture ID=NAME` | requires `ast_pattern` | validated against `defined_vars()` |

Repeating `--ast-pattern` batches independent query ids. It is not a union.

### Gaps already named in the v8 survey

`sprefa/plans/v8/2026-09-15-extract-cst-astgrep-survey.md:101-107`:

1. composed rules decode from YAML with no CLI flag; wants `--ast-rule` / `--ast-rule-file`
2. no `--family` reaches `query_source` / `query_source_facts` / `query_tree_sitter*`
3. `--ast-pattern` takes one file per invocation, no path-tagged batch stream
4. no sibling crate calls `decode_ast_rule_yaml`; exercised only by this crate's tests

### Live disjunction sites, both internal

```
tests/37_fact_matcher.rs:109,233   Op::every(&pattern).and(ops::Any::new([...]))
src/lang/prolog/_1_rehome.rs:324   Any::new
```

Neither is user-reachable.

## The two laws every version agreed on

| law | stated in | stated again in |
| --- | --- | --- |
| Capture names are output column names. Regex `(?<name>)`, ast-grep `$NAME`, tree-sitter `@name`, json `$name` all mean one column. | v1 | v6, `plans/2026-07-27-extraction-spellings.md` |
| Do not replace ast-grep. Give it what its YAML can express and its inline pattern grammar cannot. | `project_v2_ast_grep_extension.md:7` | v5, `ast_yaml` supersets `sg` |

The extraction-spellings lab adds a third that any alternation surface inherits:
the grammar tag in `{|sg:rust|| ... |}` must be a **closed set** registered at
link time, because `{|sg:rest|| ... |}` lexed fine before the fix. Closing it
forces a rename off ast-grep's short tags, since `ts` and `js` are one character
apart.

## Blockers a future attempt inherits

| blocker | source | effect |
| --- | --- | --- |
| `AstRuleRequest` strips `constraints` before the boundary | `1_ast_rule.rs` header | closes the door the `${re(A\|B\|C)}` design depended on |
| the two lowerings genuinely disagree | v6 lab, three measured divergences | any surface owes a refusal channel, not two emit functions |
| a general pattern front end needs the target parser, not just node-types.json | v6 lab §2 | schema-only grammar import is insufficient |
| brace balancing fails on real patterns | v6 lab §1 | delimiter must be explicit, `{\|sg\|\| ... \|}` not `sg{ ... }` |
| glob alternation shipped, pattern alternation did not | `pattern_parity_pipe_alternation` test exists | the two threads are separate; do not cite one as precedent for the other |

## Archive map

| root | verdict |
| --- | --- |
| `sprefa-archive-20260428` | both canonical design memos, v0-v2 era |
| `sprefa-archive-20260701` | v3/v4, holds the T2 OR task row |
| `~/projects/sprefa` | v5, v6, v7, `plans/v8` |
| `hafley-rs/crates/sprefa-extract` | today's implementation |
| `~/projects/plans` | nothing before this file |
| `~/projects/claude-research` | vendored ast-grep skills only, no original design |
| `~/projects/sprefa-archive-20260916-v7`, `~/projects/sprefa-v6` | unvisited, likely more |

Canonical files, in order of value:

1. `sprefa-archive-20260428/memory/project_metavar_grammar.md` (296 lines, 2026-04-10)
2. `sprefa-archive-20260428/memory/project_v2_ast_grep_extension.md`
3. `502456bc8:v6/prolog/labs/astgrep_patterns.md` + `.pl` (315 + 874 lines)
4. `sprefa/plans/2026-07-27-extraction-spellings.md` (571 lines)
5. `sprefa-archive-20260428/docs/labeled-branches-path-capture.md`
6. `sprefa/plans/2026-08-04-cst-native-syntax.md` (40 lines)
7. `sprefa-archive-20260701/TASKS.md` (the T2 row)

## Recovery commands

```sh
# the v6 lab, deleted from the worktree by the labs-die-on-landing protocol
cd ~/projects/sprefa
git show 502456bc8:v6/prolog/labs/astgrep_patterns.md
git show 502456bc8:v6/prolog/labs/astgrep_patterns.pl
git show 502456bc8:v6/prolog/labs/node_types_fixture.json

# the two canonical memos
cat ~/projects/sprefa-archive-20260428/memory/project_metavar_grammar.md
cat ~/projects/sprefa-archive-20260428/memory/project_v2_ast_grep_extension.md

# verify the door is still missing
cd ~/projects/hafley-rs/crates/sprefa-extract
grep -n 'Any(' src/lang/1_ast_rule.rs
grep -c 'ast_rule' src/bin/extract.rs      # expect 0
```
