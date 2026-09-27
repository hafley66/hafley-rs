; S029: per-row environment lookup of the streaming flush override.
((call_expression
  function: (scoped_identifier) @path
  arguments: (arguments (string_literal) @name)
) @hit
 (#match? @path "std::env::var_os$")
 (#match? @name "RYI_STREAM_FLUSH"))
