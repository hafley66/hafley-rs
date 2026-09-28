((statement_block) @scope
  (#emit! "scope" "span" @scope "kind" "block"))

((arrow_function) @scope
  (#emit! "scope" "span" @scope "kind" "arrow"))

((class_body) @scope
  (#emit! "scope" "span" @scope "kind" "class"))

((variable_declarator name: (identifier) @name) @declaration
  (#emit! "binding" "span" @name "name" @name "declaration" @declaration "kind" "variable"))

((assignment_expression left: (identifier) @name) @write
  (#emit! "write" "span" @name "name" @name "declaration" @write))

((identifier) @reference
  (#emit! "reference" "span" @reference "name" @reference))
