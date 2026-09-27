; S008: fs::read calls that may bypass io_path normalization.
(call_expression
  function: (scoped_identifier) @path
  (#match? @path "(^|::)fs::read$")
) @hit
