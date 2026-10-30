; x.$() read
(call_expression
  function: (member_expression
    object: (_) @target
    property: (property_identifier) @read (#eq? @read "$"))
  arguments: (arguments) @args (#eq? @args "()"))
