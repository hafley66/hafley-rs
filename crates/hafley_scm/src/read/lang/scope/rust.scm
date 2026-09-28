; Rust answers for `9_scope_rows.rs`. An `#emit!` with an absent capture emits
; nothing, so each optional field gets its own pattern fenced with `!field`.

; scope.open kind: module | block | function | closure | type
((source_file) @scope (#emit! "scope.open" "span" @scope "kind" "module"))
((block) @scope (#emit! "scope.open" "span" @scope "kind" "block"))
((function_item) @scope (#emit! "scope.open" "span" @scope "kind" "function"))
((closure_expression) @scope (#emit! "scope.open" "span" @scope "kind" "closure"))
((impl_item) @scope (#emit! "scope.open" "span" @scope "kind" "type"))
((trait_item) @scope (#emit! "scope.open" "span" @scope "kind" "type"))

; scope.decl kind: binding | cell | param | self | item | write
((let_declaration pattern: (identifier) @name type: (_) @type value: (_) @value)
  (#emit! "scope.decl" "span" @name "name" @name "type" @type "value" @value "kind" "binding"))
((let_declaration pattern: (identifier) @name type: (_) @type !value)
  (#emit! "scope.decl" "span" @name "name" @name "type" @type "kind" "binding"))
((let_declaration pattern: (identifier) @name !type value: (_) @value)
  (#emit! "scope.decl" "span" @name "name" @name "value" @value "kind" "binding"))
((let_declaration pattern: (identifier) @name !type !value)
  (#emit! "scope.decl" "span" @name "name" @name "kind" "binding"))

((parameter pattern: (identifier) @name type: (_) @type)
  (#emit! "scope.decl" "span" @name "name" @name "type" @type "kind" "param"))
((closure_parameters (identifier) @name)
  (#emit! "scope.decl" "span" @name "name" @name "kind" "param"))
((self_parameter (self) @name)
  (#emit! "scope.decl" "span" @name "name" @name "kind" "self"))

((function_item name: (identifier) @name) @value
  (#emit! "scope.decl" "span" @name "name" @name "value" @value "kind" "item"))
([
  (struct_item name: (type_identifier) @name)
  (enum_item name: (type_identifier) @name)
  (union_item name: (type_identifier) @name)
  (trait_item name: (type_identifier) @name)
  (type_item name: (type_identifier) @name)
  (const_item name: (identifier) @name)
  (static_item name: (identifier) @name)
  (mod_item name: (identifier) @name)
] @value
  (#emit! "scope.decl" "span" @name "name" @name "value" @value "kind" "item"))

; scope.callable capture: borrow | move | none. Rows merge on span; `move` wins.
((closure_expression) @callable
  (#emit! "scope.callable" "span" @callable "kind" "closure" "capture" "borrow"))
((closure_expression "move") @callable
  (#emit! "scope.callable" "span" @callable "kind" "closure" "capture" "move"))
((block (function_item name: (identifier) @name) @callable)
  (#emit! "scope.callable" "span" @callable "kind" "item" "capture" "none" "name" @name))

((closure_expression return_type: (_) @type) @callable
  (#emit! "scope.output" "span" @callable "type" @type))
((block (function_item return_type: (_) @type) @callable)
  (#emit! "scope.output" "span" @callable "type" @type))

; scope.use mode: read | exclusive | call | argument. A path segment is no local.
((identifier) @use
  (#not-has-parent? @use "scoped_identifier" "scoped_type_identifier")
  (#emit! "scope.use" "span" @use "name" @use "mode" "read"))
((self) @use
  (#not-has-parent? @use "scoped_identifier" "self_parameter")
  (#emit! "scope.use" "span" @use "name" @use "mode" "read"))
([
  (assignment_expression left: (identifier) @use)
  (compound_assignment_expr left: (identifier) @use)
  (reference_expression (mutable_specifier) value: (identifier) @use)
]
  (#emit! "scope.use" "span" @use "name" @use "mode" "exclusive"))
((call_expression function: (identifier) @use)
  (#emit! "scope.use" "span" @use "name" @use "mode" "call"))
((arguments (identifier) @use)
  (#emit! "scope.use" "span" @use "name" @use "mode" "argument"))
