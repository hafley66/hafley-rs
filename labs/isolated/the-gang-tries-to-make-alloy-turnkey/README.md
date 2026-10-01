# the-gang-tries-to-make-alloy-turnkey

Lab: generate an alloy-js language package from a tree-sitter grammar (`grammar.json` + `node-types.json`
+ `locals.scm`), then emit source with it. Alloy version `@alloy-js/core@0.23.0-dev.12`, the same as
hafley-tsp's `packages/rust`. Isolated package: `pnpm install --ignore-workspace`, then `pnpm test`.

## What moved

Origin: hafley-tsp branch `lab/alloy-turnkey`, commit `635a231`, `lab/isolated/the-gang-tries-to-make-alloy-turnkey`.

| origin | here |
|---|---|
| `0_gen.mjs` (C only, `SUBSET` inline, npm `tree-sitter-c`) | `0_gen.mjs <lang>` (language-neutral, grammar source below) |
| `c/2_print.tsx` (runtime + C whitespace + C symbols) | `core/0_print.tsx` (runtime, `Policy` interface) + `c/2_print.tsx` (C `Policy`) |
| `SUBSET` in `0_gen.mjs` | `c/0_subset.mjs` |
| `queries/locals.scm` | `c/locals.scm` |
| `gen/0_nodes.tsx` | `gen/c/0_nodes.tsx` |
| `1_demo.tsx`, `2_verify.test.tsx` | `1_demo_c.tsx`, `2_verify_c.test.tsx` (output unchanged) |
| (none) | `typespec/`, `gen/typespec/`, `1_demo_typespec.tsx`, `2_verify_typespec.test.tsx`, `3_report.mjs`, `core/1_plain.tsx` |

## Grammar source resolution (`grammarSource(lang)` in `0_gen.mjs`)

1. `cargo metadata --format-version 1` from the hafley-rs worktree root (`../../..`).
2. Package `hafley_scm`; its normal (non-dev) dependencies named `tree-sitter-*`.
3. Each such package's `manifest_path` directory is the crate dir. Grammar dirs: the crate dir when it has
   `src/grammar.json`, else each subdir with `src/grammar.json` (`tree-sitter-typescript` -> `typescript/`,
   `tsx/`; `tree-sitter-md` -> `tree-sitter-markdown/`, `tree-sitter-markdown-inline/`).
4. The language key is `grammar.json` `name` (`typescript`, `tsx`, `kotlin` from `tree-sitter-kotlin-sg`,
   `toml` from `tree-sitter-toml-ng`, `markdown_inline`, `typespec` from `crates/tree-sitter-typespec`).
5. A lang absent from that map resolves to npm `tree-sitter-<lang>` in this lab's `node_modules`.
   C is the only such lang (`tree-sitter-c@0.24.1`; hafley_scm has no C grammar).

Files read: `<grammar dir>/src/grammar.json`, `<grammar dir>/src/node-types.json`, and the first existing of
`<grammar dir>/queries/locals.scm`, `<crate dir>/queries/locals.scm`, `<lang>/locals.scm` (hand-written).

## Pipeline

| step | file | input | output |
|---|---|---|---|
| generate | `0_gen.mjs <lang>` | grammar source above, `<lang>/0_subset.mjs` | `gen/<lang>/0_nodes.tsx` |
| emit C | `1_demo_c.tsx` | hard-coded model object | `out/api.h` |
| emit TypeSpec | `1_demo_typespec.tsx` | hard-coded model object | `out/api.tsp` |
| verify C | `2_verify_c.test.tsx` | `out/api.h`, `fixtures/sample.h` | vitest results |
| verify TypeSpec | `2_verify_typespec.test.tsx` | `out/api.tsp` | vitest results |
| report | `3_report.mjs` | every hafley_scm grammar + C, all named kinds | `out/gen/<lang>/0_nodes.tsx`, markdown table |

Without `<lang>/0_subset.mjs` (or with `generate(lang, { full: true })`) every named, non-supertype kind in
`node-types.json` gets a component. Without `<lang>/2_print.tsx` the generated file imports `core/1_plain.tsx`
(one space between tokens, no symbols).

Mapping used by `0_gen.mjs`:

| tree-sitter source | alloy output |
|---|---|
| node-types.json named kind | one component (`struct_specifier` -> `StructSpecifier`) |
| node-types.json `fields` | props; `multiple` -> `Children \| Children[]`, `required` -> non-optional |
| node-types.json kind with no fields/children, no rule, or an external | leaf component, prints its children verbatim |
| kind with no rule of its own | rule = content of the first named `ALIAS` producing it |
| grammar.json `STRING` | `lit(...)` |
| `FIELD` | `field(name)`, wrapped in `opt` when its content can be `BLANK` |
| `SYMBOL` (visible, unfielded) | `child` (consumes from `props.children`) |
| `SYMBOL` to supertype / hidden choice-of-symbols | `child` |
| `SYMBOL` to other hidden `_rule` | `ref(name)` to `H_<name>`, lowered once; recursion allowed |
| `SYMBOL` to hidden external token | `blank` |
| `CHOICE` of only `STRING`s (optionally with `BLANK`) | `kw([...])` / `opt(kw([...]))`, consumes from `props.keywords` |
| `CHOICE` / `SEQ` / `REPEAT` / `REPEAT1` / `BLANK` | `choice` / `seq` / `rep(x, 0\|1)` / `blank` |
| `PREC*`, `TOKEN`, `IMMEDIATE_TOKEN`, `RESERVED` | dropped (content lowered) |
| `PATTERN` `\r?\n` | `nl` (hard line); other patterns pruned |
| SYMBOL outside the subset | pruned (`never`; collapses `CHOICE`, kills `SEQ`) |
| `locals.scm` `@local.scope` | node wraps its output in the language `Policy.scope` |
| `locals.scm` `@local.definition` on `(kind field: (leaf))` | `def` field: a string value goes through `Policy.declare`, `refkey` prop |
| `locals.scm` `@local.reference` on a leaf | leaf accepts a refkey child, resolved by `<lang>/3_SourceFile.tsx` |

Changes from the origin generator: hidden rules are emitted once as `H_<name>` and referenced with `ref`
(origin inlined them and pruned recursion); `CHOICE(STRING..., BLANK)` became `opt(kw)` (origin: `opt(lit)`,
which fewest-literals selection always dropped). The runtime guards `ref` against left recursion by
skipping a ref already active at the same queue position. The C header output is byte-identical.

Printer semantics (`core/0_print.tsx`): props become queues; the rule is run as a parser over those
queues, enumerating every way to consume them. Complete parses (all queues empty) are kept and the one
with the fewest literal tokens wins. Keyword tokens (`kw`) do not count as literals.

`Policy` (per language, `<lang>/2_print.tsx`): `space(kind, prev, tok)`, `lines` (one token per line),
`block` (`{`, indented item per line, `}`), `list` (item per line, no braces), `declare`, `scope`.

| policy | C (`c/2_print.tsx`) | TypeSpec (`typespec/2_print.tsx`) |
|---|---|---|
| default spacing | one space; none before `;` `,` `)`; none after `(` | same, plus none before `:` `?` |
| other spacing | `TIGHT` kinds: space only after `,` | `interface_member`: none after the `name` field |
| `lines` | `translation_unit` | `source_file` |
| `block` | `field_declaration_list`, `enumerator_list` | `enum_body`, `model_expression`, `interface_body` |
| `list` | (none) | `model_body` |
| scope class | `CScope`, spaces `ordinary`, `tags` | `TspScope`, space `members` |
| name policy | keyword -> trailing `_` | keyword -> backtick-quoted |

## Generated vs hand-written

| kind | file | lines |
|---|---|---|
| generator | `0_gen.mjs` | 248 |
| shared runtime | `core/0_print.tsx` | 143 |
| shared fallback policy | `core/1_plain.tsx` | 9 |
| generated C (21 components) | `gen/c/0_nodes.tsx` | 180 |
| generated TypeSpec (20 components) | `gen/typespec/0_nodes.tsx` | 181 |
| hand C: subset, name policy, scope, policy, SourceFile, locals | `c/` | 10, 23, 23, 27, 21, 9 |
| hand TypeSpec: subset, name policy, scope, policy, SourceFile, locals | `typespec/` | 8, 22, 14, 27, 21, 14 |
| demos | `1_demo_c.tsx`, `1_demo_typespec.tsx` | 102, 72 |
| verification | `2_verify_c.test.tsx`, `2_verify_typespec.test.tsx` | 207, 81 |
| report | `3_report.mjs` | 32 |
| hand-written C sample | `fixtures/sample.h` | 16 |

## Verification (`pnpm test` = generate c + typespec, `tsc -p .`, vitest)

C (unchanged from origin):
1. `out/api.h` inline snapshot.
2. tree-sitter-c (wasm, `web-tree-sitter@0.25.10`) parse: 0 `ERROR`, 0 `MISSING`, `hasError: false`.
3. `cc -std=c11 -Wall -Wextra -fsyntax-only -x c out/api.h`: exit 0, empty stderr. `zig cc` (0.16.0)
   `-c -o out/api.o` (its `-fsyntax-only` fails with `FileNotFound`): exit 0, empty stderr; cache in `.zig-cache/`.
4. Named-node kind sequence of `out/api.h` equals that of `fixtures/sample.h` (74 nodes).
5. Round trip: `fixtures/sample.h` CST -> generated components -> print -> same kind sequence and text.

TypeSpec (grammar from `crates/tree-sitter-typespec`):
1. `out/api.tsp` inline snapshot: a scalar, an enum, a model with 3 properties, an interface with 1 op.
2. `ryii query --query '(ERROR) @e' out/api.tsp` and `'(MISSING) @m'`: 0 rows each, exit 0. Binary:
   `crates/sprefa-extract/target/release/ryii` (`cargo build --release --features cli,read --bin ryii` in
   `crates/sprefa-extract`).
3. `tsp compile out/api.tsp --no-emit --warn-as-error` with boop2's `@typespec/compiler` 1.10.0: exit 0,
   "Compilation completed successfully."

The TypeSpec model passes separators explicitly: `keywords` on `ModelBody` (`;` per property),
`OperationArguments` (`,` between parameters), `EnumBody` (`,` per member), and `["op"]` on `InterfaceMember`,
since the grammar makes all of them optional anonymous strings.

## Per-language generation (`node 3_report.mjs`, full mode: every named kind)

Columns: `components` = named non-supertype kinds; `leaf_components` = kinds printing children verbatim;
`never_components` = kinds whose rule lowered to `never` (contains only pruned patterns/tokens);
`hidden_rules` = `_`-prefixed rules in grammar.json; `hidden_rules_emitted` = `H_` constants written;
`alias_count` = `ALIAS` nodes in grammar.json rules; `hidden_external_refs` = hidden external symbol uses
lowered to `blank`; `tsc_errors` = `tsc --strict` diagnostics in the generated file.

| lang | source | generator_runs | components | leaf_components | never_components | rules | hidden_rules | hidden_rules_emitted | alias_count | externals | hidden_external_refs | locals_scm_shipped | generated_lines | tsc_errors |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| c | npm tree-sitter-c 0.24.1 | yes | 125 | 27 | 6 | 182 | 29 | 5 | 110 | 0 | 0 | no | 1028 | 0 |
| commonlisp | cargo tree-sitter-commonlisp 0.4.1 | yes | 50 | 14 | 0 | 71 | 12 | 7 | 7 | 0 | 0 | no | 463 | 0 |
| gdscript | cargo tree-sitter-gdscript 6.1.0 | yes | 85 | 23 | 0 | 112 | 27 | 12 | 9 | 13 | 22 | no | 704 | 0 |
| go | cargo tree-sitter-go 0.25.0 | yes | 107 | 21 | 0 | 116 | 13 | 1 | 16 | 0 | 0 | no | 878 | 0 |
| html | cargo tree-sitter-html 0.23.2 | yes | 19 | 9 | 0 | 19 | 2 | 0 | 10 | 9 | 1 | no | 132 | 0 |
| javascript | cargo tree-sitter-javascript 0.25.0 | yes | 114 | 27 | 6 | 142 | 22 | 6 | 36 | 8 | 8 | yes | 924 | 0 |
| json | cargo tree-sitter-json 0.24.8 | yes | 12 | 7 | 0 | 14 | 2 | 1 | 0 | 0 | 0 | no | 84 | 0 |
| kotlin | cargo tree-sitter-kotlin-sg 0.4.1 | yes | 142 | 32 | 4 | 203 | 65 | 31 | 34 | 10 | 6 | no | 1112 | 0 |
| markdown | cargo tree-sitter-md 0.5.3 | yes | 51 | 26 | 0 | 80 | 49 | 32 | 30 | 47 | 47 | no | 379 | 0 |
| markdown_inline | cargo tree-sitter-md 0.5.3 | yes | 26 | 10 | 0 | 79 | 61 | 21 | 69 | 15 | 27 | no | 215 | 0 |
| prolog | cargo tree-sitter-prolog 0.1.0 | yes | 23 | 10 | 0 | 34 | 11 | 1 | 2 | 0 | 0 | no | 170 | 0 |
| python | cargo tree-sitter-python 0.25.0 | yes | 122 | 21 | 13 | 149 | 25 | 7 | 18 | 12 | 4 | no | 995 | 0 |
| rust | cargo tree-sitter-rust 0.24.2 | yes | 163 | 26 | 0 | 182 | 25 | 3 | 29 | 11 | 3 | no | 1393 | 0 |
| toml | cargo tree-sitter-toml-ng 0.7.0 | yes | 19 | 10 | 0 | 27 | 8 | 5 | 3 | 5 | 7 | no | 134 | 0 |
| tsx | cargo tree-sitter-typescript 0.23.2 | yes | 184 | 33 | 27 | 229 | 36 | 9 | 69 | 10 | 9 | yes | 1546 | 0 |
| typescript | cargo tree-sitter-typescript 0.23.2 | yes | 176 | 31 | 28 | 229 | 36 | 8 | 69 | 10 | 9 | yes | 1480 | 0 |
| typespec | cargo tree-sitter-typespec 0.0.1 | yes | 83 | 14 | 0 | 98 | 15 | 5 | 0 | 0 | 0 | no | 679 | 0 |
| yaml | cargo tree-sitter-yaml 0.7.2 | yes | 36 | 17 | 0 | 202 | 201 | 43 | 374 | 113 | 11 | no | 289 | 0 |

`locals_scm_shipped = yes` for typescript and tsx comes from the crate-root `queries/locals.scm` of
`tree-sitter-typescript`. Generation success and `tsc` are the only checks for rows other than c and typespec.

## Grammar constructs that were hard

- `ALIAS`: one visible kind is produced by several rules. C's `pointer_declarator` comes from
  `pointer_declarator`, `pointer_field_declarator`, `pointer_type_declarator`; `function_declarator`
  from 4 rules. The generator uses the rule with the kind's own name, else the first aliased content.
  Within the C subset the variants differ only in which declarator supertype they accept, so the printers
  coincide; in general they need merging. Unnamed `ALIAS` of a `PATTERN` (`#[ \t]*include` -> `#include`)
  supplies the printable text.
- Hidden `_` rules: supertypes and hidden choices of symbols are opaque `child` slots; other hidden rules
  become `H_` constants. TypeSpec's `_model_property_list` and `_enum_member_list` are right-recursive
  (`seq(item, optional(sep), optional(self))`); the origin's inlining pruned the recursion to one item.
- `PREC*`: matter only for parsing; dropped.
- `externals`: C and TypeSpec have 0. Hidden externals (Python `_newline`/`_indent`/`_dedent`, TypeScript
  `_automatic_semicolon`) lower to `blank`, so those printers emit no layout for them.
- `PATTERN`: no general inverse. Leaves take caller text; `\r?\n` maps to a newline; others are pruned.
  `never_components` counts the kinds this leaves unprintable.
- `CHOICE` ambiguity: C `sized_type_specifier` accepts `unsigned int` and `int unsigned`; with
  `keywords=["unsigned"]` and `type=int` both parses consume all props and the grammar's first alternative
  (`int unsigned`) wins. The demo uses `unsigned long` (no `type` field).
- `props.children` and `props.keywords` are untyped queues; nothing checks that a child's kind is one the
  slot accepts, or which optional string a keyword fills.
- `locals.scm` definitions are read only in the `(kind field: (leaf))` shape, so C `char *display_name`
  declares no symbol. Shipped `locals.scm` files (javascript, typescript) use other shapes; 0_gen.mjs reads
  only the patterns matching the two shapes.
- Whitespace is absent from grammar.json; it is a hand table per language.
- TypeSpec `identifier` is a non-leaf (`builtin_type | plain_identifier | backticked_identifier`), so a type
  reference is 4 components deep (`ReferenceExpression > IdentifierOrMemberExpression > Identifier > PlainIdentifier`).

## Zig

The maintained grammar is npm `@tree-sitter-grammars/tree-sitter-zig` (1.1.2, source
github.com/tree-sitter-grammars/tree-sitter-zig; the unscoped npm `tree-sitter-zig` 0.2.0 is the older
GrayJack grammar). It ships `src/grammar.json` (108 rules, 12 hidden, 0 externals, 1 inline, 4 supertypes,
117 `FIELD`, 8 `ALIAS`), `src/node-types.json`, `tree-sitter-zig.wasm`, and `queries/` with highlights,
folds, indents, injections, and no `locals.scm` or `tags.scm`. hafley_scm has no Zig crate, so it would
resolve through the npm branch of `grammarSource`. Optional unfielded keywords in `CHOICE(STRING, BLANK)`
(`pub`, `extern`, `packed`, `comptime`) now lower to `opt(kw)` and are passed through `keywords`.
Hand-written parts: name policy (keyword collisions quoted as `@"name"`), `@import` handling, the
`Policy` whitespace sets, and a scope class with one namespace per container.
