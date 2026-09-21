; Produces Kotlin CallF definition and call-site captures. The mapper joins
; definition names and receiver/callee captures to the captured owning spans.

[
  (function_declaration)
  (primary_constructor)
  (secondary_constructor)
  (lambda_literal)
] @def.span

[
  (function_declaration
    (simple_identifier) @def.name)
  (class_declaration
    (type_identifier) @def.name)
]

[
  (call_expression)
  (infix_expression)
  (additive_expression)
  (multiplicative_expression)
  (range_expression)
  (comparison_expression)
  (equality_expression)
  (check_expression)
  (prefix_expression)
  (postfix_expression)
  (indexing_expression)
  (assignment)
] @site.span

[
  (call_expression
    (simple_identifier) @site.callee)
  (call_expression
    (navigation_expression
      (navigation_suffix
        (simple_identifier) @site.callee)) @site.receiver)
]
