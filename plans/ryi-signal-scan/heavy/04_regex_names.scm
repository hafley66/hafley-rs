(call_expression
  function: [(identifier) @callee
             (member_expression property: (property_identifier) @callee)]
  (#match? @callee "^(use|to|create|get|set|on)[A-Z][A-Za-z0-9]*(Signal|Event|Service|Model|State)?$"))
