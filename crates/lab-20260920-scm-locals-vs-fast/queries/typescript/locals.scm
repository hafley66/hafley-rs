; Vendored and inheritance-expanded from:
; https://github.com/helix-editor/helix/blob/079a789e8cb08ead67f19e1971a1b7438b37354b/runtime/queries/typescript/locals.scm
; https://github.com/helix-editor/helix/blob/079a789e8cb08ead67f19e1971a1b7438b37354b/runtime/queries/_typescript/locals.scm
; https://github.com/helix-editor/helix/blob/079a789e8cb08ead67f19e1971a1b7438b37354b/runtime/queries/ecma/locals.scm
; Upstream commit: 079a789e8cb08ead67f19e1971a1b7438b37354b
; This Source Code Form is subject to the terms of the Mozilla Public
; License, v. 2.0. If a copy of the MPL was not distributed with this
; file, You can obtain one at https://mozilla.org/MPL/2.0/.

; Scopes from _typescript and ecma.
[
  (type_alias_declaration)
  (class_declaration)
  (interface_declaration)
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

; Definitions from _typescript and ecma.
(type_parameter
  name: (type_identifier) @local.definition.type.parameter)

(required_parameter
  (identifier) @local.definition.variable.parameter)

(optional_parameter
  (identifier) @local.definition.variable.parameter)

(arrow_function
  parameter: (identifier) @local.definition.variable.parameter)

; References from _typescript and ecma.
(type_identifier) @local.reference
(identifier) @local.reference

; Local extensions: declarations, calls, imports, and exports.
(function_declaration
  name: (identifier) @local.definition.function)

(method_definition
  name: (property_identifier) @local.definition.function)

(class_declaration
  name: (type_identifier) @local.definition.type)

(variable_declarator
  name: (identifier) @local.definition.variable)

[
  (call_expression
    function: (identifier) @local.call)
  (call_expression
    function: (member_expression
      property: (property_identifier) @local.call))
  (new_expression
    constructor: (identifier) @local.call)
]

(import_statement) @local.import
(export_statement) @local.export.package
