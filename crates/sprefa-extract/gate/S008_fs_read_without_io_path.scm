; S008: fs::read calls that may bypass io_path normalization.
(call_expression
  function: (scoped_identifier) @callee
  (#match? @callee "(^|::)fs::read$")
) @hit
