; @comment-ok: the vendored MPL-2.0 header below travels with the file.
; Vendored and inheritance-expanded from:
; https://github.com/helix-editor/helix/blob/079a789e8cb08ead67f19e1971a1b7438b37354b/runtime/queries/rust/locals.scm
; Upstream commit: 079a789e8cb08ead67f19e1971a1b7438b37354b
; This Source Code Form is subject to the terms of the Mozilla Public
; License, v. 2.0. If a copy of the MPL was not distributed with this
; file, You can obtain one at https://mozilla.org/MPL/2.0/.

; Scopes.
[
  (function_item)
  (struct_item)
  (enum_item)
  (union_item)
  (type_item)
  (trait_item)
  (impl_item)
  (mod_item)
  (closure_expression)
  (block)
  (for_expression)
  (match_arm)
] @local.scope

; Definitions. The outer capture is the span L1 selects; the inner captures
; carry the helix vocabulary.
[
  (function_item
    name: (identifier) @local.definition.function)
  (struct_item
    name: (type_identifier) @local.definition.type)
  (enum_item
    name: (type_identifier) @local.definition.type)
  (union_item
    name: (type_identifier) @local.definition.type)
  (trait_item
    name: (type_identifier) @local.definition.type)
  (type_item
    name: (type_identifier) @local.definition.type)
  (const_item
    name: (identifier) @local.definition.variable)
  (static_item
    name: (identifier) @local.definition.variable)
  (let_declaration
    pattern: (identifier) @local.definition.variable)
  (parameter
    pattern: (identifier) @local.definition.variable.parameter)
  (type_parameters
    (type_parameter
      name: (type_identifier) @local.definition.type.parameter))
] @local.def.span

; References. A `field_identifier` is not one: `a.b` names a member.
[
  (identifier)
  (type_identifier)
] @local.reference

; Call sites, under the same outer span capture as the definitions. A call
; through a path (`std::fs::read_to_string`) is qualified, never a free name.
[
  (call_expression
    function: (identifier) @local.call)
  (call_expression
    function: (field_expression
      field: (field_identifier) @local.call))
] @local.site.span

(use_declaration) @local.import

; The item a `pub` hands out, whole, so the definition it names sits inside it.
[
  (function_item (visibility_modifier))
  (struct_item (visibility_modifier))
  (enum_item (visibility_modifier))
  (union_item (visibility_modifier))
  (trait_item (visibility_modifier))
  (type_item (visibility_modifier))
  (const_item (visibility_modifier))
  (static_item (visibility_modifier))
  (mod_item (visibility_modifier))
] @local.export.package
