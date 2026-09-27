; S144: windows(literal.len()) can panic when the literal is empty.
((call_expression
  function: (field_expression field: (field_identifier) @method)
  arguments: (arguments
    (call_expression
      function: (field_expression
        value: (identifier) @receiver
        field: (field_identifier) @inner_method
      )
    )
  )
) @hit
 (#eq? @method "windows")
 (#eq? @receiver "literal")
 (#eq? @inner_method "len"))
