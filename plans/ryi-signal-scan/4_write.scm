; x.$(value) write
(call_expression
  function: (member_expression
    object: (_) @target
    property: (property_identifier) @write (#eq? @write "$"))
  arguments: (arguments . (_) @value))
