# lane-scm-relations-doc RESULT

Deliverable: `crates/sprefa-extract/docs/2_scm-with-ast-grep-relations-20260920.md`, 311 lines. Base `22fdedaa`. No Rust changed. `examples/` holds its five tracked files; the throwaway `examples/scm_doc_probe.rs` was removed with `rm examples/scm_doc_probe.rs`.

## file:line references verified

26 unique references. Each line was opened and compared with the claim in the guide.

| reference | line text |
| --- | --- |
| `src/lang/1_ast_rule.rs:180` | `/// A kind: string the target grammar does not spell. ast-grep matches` |
| `src/lang/1_ast_rule.rs:182` | `UnknownKind { kind: String, language: String },` |
| `src/lang/1_ast_rule.rs:21` | `pub enum AstRule {` |
| `src/lang/1_ast_rule.rs:25` | `Matches(String),` |
| `src/lang/1_ast_rule.rs:321` | `unknown_kind(&request.rule, language)` |
| `src/lang/1_ast_rule.rs:53` | `pub enum StopBy {` |
| `src/lang/2_source_query.rs:161` | `fn query_language(name: &str) -> Result<tree_sitter::Language, String> {` |
| `src/lang/5_scm_lower.rs:1` | `//! .scm surface syntax lowered into [AstRule]. The query text is parsed by` |
| `src/lang/5_scm_lower.rs:100` | `for definition in top_level_definitions(root) {` |
| `src/lang/5_scm_lower.rs:126` | `let rule = match lowered.len() {` |
| `src/lang/5_scm_lower.rs:202` | `fn top_level_label(definition: Node<'_>, source: &str) -> Option<String> {` |
| `src/lang/5_scm_lower.rs:226` | `fn lower_definition(` |
| `src/lang/5_scm_lower.rs:254` | `if host.id() != root.id() {` |
| `src/lang/5_scm_lower.rs:310` | `/// A parameter is capture \| identifier \| string, never a nested rule.` |
| `src/lang/5_scm_lower.rs:33` | `//! # What the surface carries that AstRule cannot` |
| `src/lang/5_scm_lower.rs:334` | `if !(2..=3).contains(&parameters.len()) {` |
| `src/lang/5_scm_lower.rs:343` | `let (name, negated) = match name.strip_prefix("not-") {` |
| `src/lang/5_scm_lower.rs:352` | `let relation: fn(Box<AstRule>, Option<StopBy>) -> AstRule = match name.as_str() {` |
| `src/lang/5_scm_lower.rs:371` | `let reference = reference_argument(parameters[1], source, labels)?;` |
| `src/lang/5_scm_lower.rs:372` | `// A third argument is a STRING for the walk mode and an IDENTIFIER for a` |
| `src/lang/5_scm_lower.rs:51` | `pub struct ScmProgram {` |
| `src/lang/5_scm_lower.rs:57` | `pub enum ScmLowerError {` |
| `src/lang/5_scm_lower.rs:84` | `pub fn lower_scm(text: &str) -> Result<ScmProgram, ScmLowerError> {` |
| `src/lang/extract_lang.rs:23` | `pub enum RyiLang {` |
| `src/lang/extract_lang.rs:36` | `pub fn from_path(path: &str) -> Option<Self> {` |
| `src/lang/extract_lang.rs:54` | `pub fn parse_name(name: &str) -> Option<Self> {` |

## Validation commands

| command | output |
| --- | --- |
| `cargo run --example scm_vs_yaml --features cli -- src/project.rs` | exit 0; `rule trees equal: true`, `utils equal: true`, `matches .scm: 16`, `matches yaml: 16`, `match sets equal: true`, `1-1` |
| `grep -c '' docs/2_scm-with-ast-grep-relations-20260920.md` | `311` |
| `grep -nE ' — \|not [A-Za-z]+, [a-z]' docs/2_scm-with-ast-grep-relations-20260920.md` | no output, exit 1 |
| `grep -oE '[0-9a-z_/]+\.rs:[0-9]+' <guide> \| sort -u \| wc -l` | `26` |
| `linkcheck.py <guide>` (writing-documentation skill) | `0 links checked, 0 problem(s)`; the guide holds no markdown link |

## Own run, `#inside?` / `#not-inside?` over `src/project.rs`

| query | matches |
| --- | --- |
| `((call_expression) @m (#match? @m ""))` | 1121 |
| `(#inside? @m closure)` | 282 |
| `(#not-inside? @m closure)` | 839 |
| sum | 1121 |
| `(#inside? @m closure "neighbor")` | 70 |

The same probe printed every lowered rule in the predicate, stop-by and drop tables, and every literal error message in the refusal table.

## Upstream facts checked

| fact | check |
| --- | --- |
| `tree-sitter#880` open since 2021-01-13 | `gh api repos/tree-sitter/tree-sitter/issues/880` -> `open`, `2021-01-13T20:25:06Z`, title "Specify descendant or ancestor in query" |
| `QueryPredicateArg` is `Capture(u32) \| String(Box<str>)` | `tree-sitter-0.25.10/binding_rust/lib.rs:302`, the version in `Cargo.lock` |
| `parameters` children are `capture`, `identifier`, `string` | `tree-sitter-tsquery-0.8.0/src/node-types.json` |
| `ts_query__parse_predicate` exists | `tree-sitter-0.25.10/src/query.c:2084` |
| 23 `SupportLang` members | `ast-grep-language-0.38.7/src/lib.rs:267` `all_langs()`; plus 5 `RyiLang` variants at `src/lang/extract_lang.rs:23` gives 28 |

## Deviations from the brief

- The brief cites `5_scm_lower.rs:367-382` for recursion by name. The comment is at `:310`, the reference read at `:371`. The guide cites those.
- The brief places `query_language` beside `extract_lang.rs`. It lives at `src/lang/2_source_query.rs:161` and serves the native tree-sitter query path. `query_ast_rule` picks its grammar through `RyiLang::from_path`. The guide states both.
- The existing two docs hard-wrap and number sections. The brief's house style wins, so the guide does neither.

## Sentences written without a run behind them

- The walk semantics of `End`, `Rule` and absent `stop_by`, and the YAML default of neighbor, come from ast-grep's documentation. The 282 versus 70 counts agree with them. No run exercised `StopBy::Rule`, `Follows` or `Precedes` against source text; only their lowering was run.
- "`query.c` accepts any identifier ending in `?` or `!`": the function was located, its body was taken from the brief's fact table.
- The lowered output in "Refer to another pattern by name" was derived from the source and from the `scm_vs_yaml` output, which holds the same `Inside` node. That exact two-line file was never lowered in a run.
- The Rust grammar spelling `arguments` as both field and node type was stated from knowledge of tree-sitter-rust. Only the lowering of `(call_expression !arguments)` was run.

## Sentences wanted and left out

- That a labelled pattern can carry its own predicates and so refer to another label. The source permits it (`lower_definition` lowers `utils` entries too). No run confirmed it, so the guide omits it.
- Whether `#match?` anchors the regex to the whole node text. Unverified, omitted.
