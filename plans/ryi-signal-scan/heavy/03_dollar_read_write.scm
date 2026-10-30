[
  (call_expression
    function: (member_expression property: (property_identifier) @read (#eq? @read "$"))
    arguments: (arguments) @none (#eq? @none "()"))
  (call_expression
    function: (member_expression property: (property_identifier) @write (#eq? @write "$"))
    arguments: (arguments . (_) @value))
]
