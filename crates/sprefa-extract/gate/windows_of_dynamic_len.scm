; ledger: S144 windows(x.len()) calls
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
 (#eq? @inner_method "len"))
