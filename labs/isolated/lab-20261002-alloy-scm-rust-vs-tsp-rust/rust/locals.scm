(source_file) @local.scope
(block) @local.scope
(declaration_list) @local.scope
(struct_item name: (type_identifier) @local.definition)
(enum_item name: (type_identifier) @local.definition)
(trait_item name: (type_identifier) @local.definition)
(type_item name: (type_identifier) @local.definition)
(function_item name: (identifier) @local.definition)
(function_signature_item name: (identifier) @local.definition)
(const_item name: (identifier) @local.definition)
(static_item name: (identifier) @local.definition)
(enum_variant name: (identifier) @local.definition)
(field_declaration name: (field_identifier) @local.definition)
(type_parameter name: (type_identifier) @local.definition)
(associated_type name: (type_identifier) @local.definition)
(identifier) @local.reference
(type_identifier) @local.reference
