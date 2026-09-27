; SPDX-License-Identifier: MPL-2.0
; Copyright (c) Helix contributors.
; Source: https://github.com/helix-editor/helix/blob/master/runtime/queries/_typescript/locals.scm

; Scopes
;-------

[
  (type_alias_declaration)
  (class_declaration)
  (interface_declaration)
] @local.scope

; Definitions
;------------

(type_parameter
  name: (type_identifier) @local.definition.type.parameter)

; Javascript and Typescript Treesitter grammars deviate when defining the
; tree structure for parameters, so we need to address them in each specific
; language instead of ecma.

; (i: t)
; (i: t = 1)
(required_parameter
  (identifier) @local.definition.variable.parameter)
; (i?: t)
; (i?: t = 1) // Invalid but still possible to highlight.
(optional_parameter
  (identifier) @local.definition.variable.parameter)

; References
;-----------

(type_identifier) @local.reference
(identifier) @local.reference
