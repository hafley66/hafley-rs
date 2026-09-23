; @comment-ok: the vendored MPL-2.0 header below travels with the file.
; Vendored from:
; https://github.com/helix-editor/helix/blob/079a789e8cb08ead67f19e1971a1b7438b37354b/runtime/queries/kotlin/locals.scm
; Upstream commit: 079a789e8cb08ead67f19e1971a1b7438b37354b
; This Source Code Form is subject to the terms of the Mozilla Public
; License, v. 2.0. If a copy of the MPL was not distributed with this
; file, You can obtain one at https://mozilla.org/MPL/2.0/.

; Scopes
[
  (class_declaration)
  (function_declaration)
  (lambda_literal)
  ; `fun(x) { … }` expression form: has its own parameters and body.
  (anonymous_function)
  (control_structure_body)
  (when_entry)
  ; for/while loop variables are declared on the statement, not in its body.
  (for_statement)
] @local.scope

; Definitions. The outer capture is the span L1 selects; the inner captures
; carry the helix vocabulary the scope tree reads.
[
  (type_parameter
    (type_identifier) @local.definition.type.parameter)
  (parameter
    (simple_identifier) @local.definition.variable.parameter)
  (lambda_literal
    (lambda_parameters
      (variable_declaration
        (simple_identifier) @local.definition.variable.parameter)))
  (variable_declaration
    (simple_identifier) @local.definition.variable)
  (function_declaration
    (simple_identifier) @local.definition.function)
  (class_declaration
    (type_identifier) @local.definition.type)
  (object_declaration
    (type_identifier) @local.definition.type)
  (type_alias
    (type_identifier) @local.definition.type)
] @local.def.span

; References
[
  (simple_identifier)
  (type_identifier)
  (interpolated_identifier)
] @local.reference

; Member access after `.` is not a local reference.
(navigation_suffix
  (simple_identifier) @_)

; Call sites, under the same outer span capture as the definitions.
[
  (call_expression
    (simple_identifier) @local.call)
  (call_expression
    (navigation_expression
      (navigation_suffix
        (simple_identifier) @local.call)))
] @local.site.span

(package_header) @local.export.package
(package_header (identifier) @module.package)
(import_header) @local.import

; Import spellings consumed by CallF, module resolution, and rehome.
(import_header
  (identifier)? @import.path
  (wildcard_import)? @import.wildcard
  (import_alias (type_identifier) @import.alias)?) @import.span

; ── TypeF entities ──────────────────────────────────────────────────────────
; The generic class match also sees interfaces and enums. Their more specific
; matches override its kind for the same declaration span.
((class_declaration
    (type_identifier) @type.name) @type.span
  (#set! "type.kind" "class")
  (#set! "type.form" "declaration"))
((class_declaration
    "interface"
    (type_identifier) @type.name) @type.span
  (#set! "type.kind" "interface")
  (#set! "type.form" "declaration"))
((class_declaration
    "enum"
    (type_identifier) @type.name) @type.span
  (#set! "type.kind" "enum")
  (#set! "type.form" "declaration"))
((object_declaration
    (type_identifier) @type.name) @type.span
  (#set! "type.kind" "class")
  (#set! "type.form" "declaration"))
((companion_object
    (type_identifier) @type.name) @type.span
  (#set! "type.kind" "class")
  (#set! "type.form" "companion"))
((function_declaration
    (simple_identifier) @type.name) @type.span
  (#set! "type.kind" "function")
  (#set! "type.form" "function"))

; ── CallF ───────────────────────────────────────────────────────────────────
; Hand-written, not vendored: the vendored helix captures end above.
; 6_scm_family.rs reads def.* and emitted sites; 7_scm_rows.rs reads local.*.

((function_declaration
    (simple_identifier) @def.name) @def.span @def.scope
  (#set! "call.def" "function")
  (#set! "call.scope" "free"))
((function_declaration
    (function_body) @def.body) @def.span
  (#set! "call.def" "function"))
((primary_constructor) @def.span
  (#set! "call.def" "constructor"))
((secondary_constructor) @def.span
  (#set! "call.def" "constructor"))
((lambda_literal) @def.span
  (#has-ancestor? @def.span "function_declaration")
  (#set! "call.def" "lambda"))

((class_declaration
    (type_identifier) @def.name) @def.scope
  (#set! "call.scope" "method"))
((object_declaration) @def.scope
  (#set! "call.scope" "method"))

((call_expression
    (simple_identifier) @site.callee) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.callee "callee" @site.callee))
((call_expression
    (navigation_expression
      (navigation_suffix
        (simple_identifier) @site.callee)) @site.receiver) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.receiver "callee" @site.callee))
((infix_expression
    (simple_identifier) @site.callee) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.callee "callee" @site.callee))

((call_expression
    (call_expression)
    (call_suffix) @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "invoke"))
((indexing_expression
    (indexing_suffix) @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "get"))
((assignment
    (directly_assignable_expression
      (indexing_suffix) @site.operator)) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "set"))

; Operator spellings are Kotlin's call-site names. The match keeps the
; expression span for the existing ordered CallF projection; the token is the
; emitted site's span. The host emission keeps these rows in the match arena.
((additive_expression "+" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "plus"))
((additive_expression "-" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "minus"))
((multiplicative_expression "*" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "times"))
((multiplicative_expression "/" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "div"))
((multiplicative_expression "%" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "rem"))
((range_expression ".." @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "rangeTo"))
((range_expression "..<" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "rangeUntil"))
((equality_expression "==" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "equals"))
((equality_expression "!=" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "equals"))
((comparison_expression "<" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "compareTo"))
((comparison_expression ">" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "compareTo"))
((comparison_expression "<=" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "compareTo"))
((comparison_expression ">=" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "compareTo"))
((check_expression "in" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "contains"))
((prefix_expression "-" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "unaryMinus"))
((prefix_expression "+" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "unaryPlus"))
((prefix_expression "!" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "not"))
((prefix_expression "++" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "inc"))
((prefix_expression "--" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "dec"))
((postfix_expression "++" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "inc"))
((postfix_expression "--" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "dec"))
((assignment "+=" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "plusAssign"))
((assignment "-=" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "minusAssign"))
((assignment "*=" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "timesAssign"))
((assignment "/=" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "divAssign"))
((assignment "%=" @site.operator) @site.span
  (#emit! "call.site" "group" @site.span "span" @site.operator "callee" "remAssign"))
