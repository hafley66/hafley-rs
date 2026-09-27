; S029: std::env::var/var_os calls in ryi command and edit functions.
(call_expression
  function: (scoped_identifier) @callee
  (#match? @callee "std::env::var(_os)?$")
) @hit
