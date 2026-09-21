# SCM locals versus fast

## Kotlin judge

The judge keys edges as `(caller_path, caller_name) -> (callee_path, callee_name)`.
Duplicate rows collapse in the sets. The current `ryi fast` binary produces 9
receiver keys and 6 module keys under that identity.

| fixture | both | lab-only | fast-only |
| --- | ---: | ---: | ---: |
| `kotlin_receivers` edges | 5 | 1 | 4 |
| `kotlin_receivers` unresolved | 0 | 2 | 1 |
| `kotlin_module_resolve` edges | 6 | 3 | 0 |
| `kotlin_module_resolve` unresolved | 0 | 2 | 4 |

Every disagreement and its cause:

- Receiver fast-only `boundLeg -> project`, `ctorLeg -> run`, `fieldLeg -> run`,
  and `paramLeg -> run`: `locals.scm` supplies names and lexical scopes but no
  receiver type. The graph has no edge from a parameter, field, constructor
  result, or type bound into the member scope.
- Receiver lab-only `shadow -> run`: the lexical parameter definition wins.
  `ryi fast` applies the untyped-receiver decline and emits unresolved
  `run/inferred` instead.
- Receiver unresolved lab-only `project/no_graph_path` and
  `run/no_graph_path`: these are the four missing typed receiver edges collapsed
  by `(path, name, reason)`. The fast unresolved set contains only the shadowed
  `run/inferred` site.
- Module lab-only `main -> App.kt:dupName` and `main -> Helper.kt:dupName`: the
  lab links package roots and returns every reachable definition. `ryi fast`
  declines the two-definition package name as `dupName/ambiguous`.
- Module lab-only `<root> -> Widget`: the query captures the constructor call
  in the top-level property initializer. `ryi fast` reports the same spelling
  as `Widget/ambiguous` in its unresolved set.
- Module unresolved lab-only `build/no_graph_path`: the query captures the
  import alias spelling, while the graph stores the declaration as
  `makeWidget`; no alias rewrite node is present.
- Module unresolved `println`: both engines decline the external symbol. The
  lab reason is `no_graph_path`; the fast reason is `no_corpus_def`.
- Module fast-only unresolved `plus/no_corpus_def`: the locals query captures
  named calls and does not mint Kotlin operator lowering calls.

## TypeScript judge

The full `ts5_findings/module_plane` fixture has 0 shared edge keys, 1 lab-only
key, and 9 fast-only keys. The first whole-corpus lab run hit the 10-second
limit in recursive traversal across the 24-file import graph. The recorded
judge runs each file independently and unions the results.

Every disagreement and its cause:

- The 8 cross-file fast-only keys (`normalize`, `widen`, `fromB`, `theDefault`,
  `member`, `inner`, exported `isIdentifier`, and `deep`) require import,
  re-export, alias, namespace, default-export, or multi-hop module edges. The
  per-file run contains no target file root.
- Fast-only `parse -> isIdentifier` and lab-only `<root> -> isIdentifier` name
  the same same-file target. The merged Helix query marks the statement block
  as the innermost scope; the lab owner projection does not associate that
  nested scope with its containing function name.
