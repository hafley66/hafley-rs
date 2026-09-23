# Query emissions

`#emit!` attaches a relation row to each accepted tree-sitter query match.
The first argument is the relation name; remaining arguments are field-name / 
value pairs. A value is a capture's byte span or a query-owned string literal.

```scheme
((call_expression (identifier) @callee) @group
  (#emit! "call.site" "group" @group "span" @callee "callee" @callee))

((binary_expression "+" @operator) @group
  (#emit! "call.site" "group" @group "span" @operator "callee" "plus"))
```

`build` checks pair arity and duplicate keys, and interns relation, field and
literal strings once. `run` appends an `EmittedFact` plus its fields to the
arena. A missing capture skips that fact. The arena owns only file IDs, field
IDs, byte ranges and literal IDs; the source and query retain their lifetimes.

Kotlin's CallF projector groups `call.site` rows by expression span. A
captured callee takes precedence over literal operator names in that group.
