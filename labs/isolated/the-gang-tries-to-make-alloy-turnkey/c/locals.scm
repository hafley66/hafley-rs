; tree-sitter-c 0.24.1 ships no locals.scm; hand-written for this lab (read by 0_gen.mjs).
(translation_unit) @local.scope
(field_declaration_list) @local.scope
(type_definition declarator: (type_identifier) @local.definition)
(struct_specifier name: (type_identifier) @local.definition)
(enum_specifier name: (type_identifier) @local.definition)
(enumerator name: (identifier) @local.definition)
(field_declaration declarator: (field_identifier) @local.definition)
(type_identifier) @local.reference
