; SPDX-License-Identifier: MPL-2.0
; Copyright (c) Helix contributors.
; Source: https://github.com/helix-editor/helix/blob/master/runtime/queries/ecma/locals.scm

; Scopes
;-------

[
  (statement_block)
  (arrow_function)
  (function_expression)
  (function_declaration)
  (method_definition)
  (for_statement)
  (for_in_statement)
  (catch_clause)
  (finally_clause)
] @local.scope

; Definitions
;------------

; i => ...
(arrow_function
  parameter: (identifier) @local.definition.variable.parameter)

; References
;------------

(identifier) @local.reference
