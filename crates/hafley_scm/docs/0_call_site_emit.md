# Call-site emission

`#emit-call-site!` is a host instruction in a tree-sitter query. It takes three
arguments: the expression capture used to group competing matches, the byte
span to report, and either a capture containing the callee text or a literal
callee name.

```scheme
((call_expression (simple_identifier) @callee) @group
  (#emit-call-site! @group @callee @callee))

((additive_expression "+" @operator) @group
  (#emit-call-site! @group @operator "plus"))
```

`build` checks argument count and types. Each accepted match appends one
`EmittedCallSite` to the run's `MatchArena`. The record stores a file index,
byte ranges, and an optional index into literals owned by `QueryExt`. Source
text and query literals stay outside the arena. A match missing a required
capture emits no record.

Kotlin's CallF projection groups records by expression span. A captured
callee takes precedence over operator names in the same expression. The
projector interns the selected name into its own `Strings` table.
