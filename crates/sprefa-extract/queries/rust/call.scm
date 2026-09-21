; CallF defs. Pattern order is the projection's precedence: the first pattern to
; claim a def span wins, so an impl/trait method is a method before it is a fn.

(impl_item
  type: (_) @owner.self
  body: (declaration_list
    (function_item
      name: (identifier) @def.name
      body: (block) @def.body) @def.method))

(impl_item
  trait: (_) @owner.trait
  type: (_) @owner.self
  body: (declaration_list
    (function_item
      name: (identifier) @def.name
      body: (block) @def.body) @def.method))

(trait_item
  name: (type_identifier) @owner.trait
  body: (declaration_list
    (function_item
      name: (identifier) @def.name
      body: (block) @def.body) @def.method))

(trait_item
  name: (type_identifier) @owner.trait
  body: (declaration_list
    (function_signature_item
      name: (identifier) @def.name) @def.method @def.sig))

(function_item
  name: (identifier) @def.name
  body: (block) @def.body) @def.free

(enum_item
  body: (enum_variant_list
    (enum_variant
      name: (identifier) @def.name @def.variant)))

(closure_expression) @def.lambda
