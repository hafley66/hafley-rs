// Node kinds 0_gen.mjs emits components for (generator input).
export const SUBSET = [
  "translation_unit", "preproc_include", "type_definition", "struct_specifier",
  "field_declaration_list", "field_declaration", "enum_specifier", "enumerator_list",
  "enumerator", "primitive_type", "type_identifier", "field_identifier",
  "sized_type_specifier", "pointer_declarator", "function_declarator", "parameter_list",
  "parameter_declaration", "declaration",
  // added: needed for `(*op)(void *self)` and `<stdint.h>`
  "parenthesized_declarator", "identifier", "system_lib_string",
];
