; S066: scoped Default calls other than the literal Default::default spelling.
(call_expression
  function: (scoped_identifier) @path
  (#not-eq? @path "Default::default")
) @hit
