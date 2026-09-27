; SPDX-License-Identifier: MPL-2.0
; Copyright (c) Helix contributors.
; Source: https://github.com/helix-editor/helix/blob/master/runtime/queries/kotlin/locals.scm
; Upstream Helix locals query, extended below for this lab's calls and imports.

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

; Definitions
(type_parameter
  (type_identifier) @local.definition.type.parameter)

(parameter
  (simple_identifier) @local.definition.variable.parameter)
(lambda_literal
  (lambda_parameters
    (variable_declaration
      (simple_identifier) @local.definition.variable.parameter)))

; Loop and local `val`/`var` bindings; defined so inner references resolve and
; shadow correctly.
(variable_declaration
  (simple_identifier) @local.definition.variable)

; References
(simple_identifier) @local.reference
(type_identifier) @local.reference
(interpolated_identifier) @local.reference
; Member access after `.` is not a local reference.
(navigation_suffix
  (simple_identifier) @_)

; Lab additions: callable names participate in local resolution.
(call_expression
  (simple_identifier) @local.reference)

; Lab additions: top-level names and package headers support corpus linking.
(function_declaration
  (simple_identifier) @local.definition.namespace)
(class_declaration
  (type_identifier) @local.definition.type)
(package_header) @local.package
(import_header) @local.import

; Lab additions: imported paths are references; aliases introduce names.
(import_header
  (identifier) @local.reference)
(import_alias
  (type_identifier) @local.definition.namespace)
