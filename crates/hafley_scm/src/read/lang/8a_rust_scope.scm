((block) @scope
  (#emit! "scope" "span" @scope "kind" "block"))

((closure_expression) @scope
  (#emit! "scope" "span" @scope "kind" "closure"))

((let_declaration pattern: (identifier) @name) @declaration
  (#emit! "binding" "span" @name "name" @name "declaration" @declaration "kind" "let"))

((parameter pattern: (identifier) @name) @declaration
  (#emit! "binding" "span" @name "name" @name "declaration" @declaration "kind" "parameter"))

((identifier) @reference
  (#emit! "reference" "span" @reference "name" @reference))
