; TypeScript answers for `9_scope_rows.rs`. An `#emit!` with an absent capture
; emits nothing, so each optional field gets its own pattern fenced with `!field`.

; scope.open kind: module | block | function | closure | type
((program) @scope (#emit! "scope.open" "span" @scope "kind" "module"))
((statement_block) @scope (#emit! "scope.open" "span" @scope "kind" "block"))
([
  (function_declaration)
  (function_expression)
  (generator_function_declaration)
  (method_definition)
] @scope
  (#emit! "scope.open" "span" @scope "kind" "function"))
((arrow_function) @scope (#emit! "scope.open" "span" @scope "kind" "closure"))
((class_body) @scope (#emit! "scope.open" "span" @scope "kind" "type"))

; scope.decl kind: binding | cell | param | self | item | write
; `let`/`const` never redeclare in one scope; `var` is one hoisted cell.
((lexical_declaration (variable_declarator name: (identifier) @name type: (type_annotation (_) @type) value: (_) @value))
  (#emit! "scope.decl" "span" @name "name" @name "type" @type "value" @value "kind" "binding"))
((lexical_declaration (variable_declarator name: (identifier) @name type: (type_annotation (_) @type) !value))
  (#emit! "scope.decl" "span" @name "name" @name "type" @type "kind" "binding"))
((lexical_declaration (variable_declarator name: (identifier) @name !type value: (_) @value))
  (#emit! "scope.decl" "span" @name "name" @name "value" @value "kind" "binding"))
((lexical_declaration (variable_declarator name: (identifier) @name !type !value))
  (#emit! "scope.decl" "span" @name "name" @name "kind" "binding"))
((variable_declaration (variable_declarator name: (identifier) @name type: (type_annotation (_) @type) value: (_) @value))
  (#emit! "scope.decl" "span" @name "name" @name "type" @type "value" @value "kind" "cell"))
((variable_declaration (variable_declarator name: (identifier) @name type: (type_annotation (_) @type) !value))
  (#emit! "scope.decl" "span" @name "name" @name "type" @type "kind" "cell"))
((variable_declaration (variable_declarator name: (identifier) @name !type value: (_) @value))
  (#emit! "scope.decl" "span" @name "name" @name "value" @value "kind" "cell"))
((variable_declaration (variable_declarator name: (identifier) @name !type !value))
  (#emit! "scope.decl" "span" @name "name" @name "kind" "cell"))

([
  (required_parameter pattern: (identifier) @name type: (type_annotation (_) @type))
  (optional_parameter pattern: (identifier) @name type: (type_annotation (_) @type))
]
  (#emit! "scope.decl" "span" @name "name" @name "type" @type "kind" "param"))
([
  (required_parameter pattern: (identifier) @name !type)
  (optional_parameter pattern: (identifier) @name !type)
  (arrow_function parameter: (identifier) @name)
]
  (#emit! "scope.decl" "span" @name "name" @name "kind" "param"))

([
  (function_declaration name: (identifier) @name)
  (generator_function_declaration name: (identifier) @name)
  (class_declaration name: (type_identifier) @name)
  (method_definition name: (property_identifier) @name)
] @value
  (#emit! "scope.decl" "span" @name "name" @name "value" @value "kind" "item"))

; A reassignment is a new version of the cell it names, never a declaration.
([
  (assignment_expression left: (identifier) @name right: (_) @value)
  (augmented_assignment_expression left: (identifier) @name right: (_) @value)
]
  (#emit! "scope.decl" "span" @name "name" @name "value" @value "kind" "write"))
((update_expression argument: (identifier) @name)
  (#emit! "scope.decl" "span" @name "name" @name "kind" "write"))

; scope.callable capture: every TS callable captures the cell.
((arrow_function) @callable
  (#emit! "scope.callable" "span" @callable "kind" "closure" "capture" "cell"))
((function_expression) @callable
  (#emit! "scope.callable" "span" @callable "kind" "function" "capture" "cell"))
((statement_block (function_declaration name: (identifier) @name) @callable)
  (#emit! "scope.callable" "span" @callable "kind" "function" "capture" "cell" "name" @name))
((method_definition name: (property_identifier) @name) @callable
  (#emit! "scope.callable" "span" @callable "kind" "method" "capture" "cell" "name" @name))

([
  (arrow_function return_type: (type_annotation (_) @type))
  (function_expression return_type: (type_annotation (_) @type))
  (function_declaration return_type: (type_annotation (_) @type))
  (method_definition return_type: (type_annotation (_) @type))
] @callable
  (#emit! "scope.output" "span" @callable "type" @type))
([
  (arrow_function "async")
  (function_expression "async")
  (function_declaration "async")
  (method_definition "async")
] @callable
  (#emit! "scope.async" "span" @callable))

; scope.use mode: read | call | argument | self | frame
([
  (identifier)
  (shorthand_property_identifier)
] @use
  (#emit! "scope.use" "span" @use "name" @use "mode" "read"))
((call_expression function: (identifier) @use)
  (#emit! "scope.use" "span" @use "name" @use "mode" "call"))
((arguments (identifier) @use)
  (#emit! "scope.use" "span" @use "name" @use "mode" "argument"))
((this) @use
  (#emit! "scope.use" "span" @use "name" @use "mode" "self"))
((identifier) @use
  (#eq? @use "arguments")
  (#emit! "scope.use" "span" @use "name" @use "mode" "frame"))
((await_expression) @use
  (#emit! "scope.use" "span" @use "name" "await" "mode" "frame"))
