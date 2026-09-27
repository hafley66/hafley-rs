; S171: include_str!/include_bytes! paths traverse up at least two directories.
((macro_invocation
  macro: (identifier) @macro
) @hit
 (#match? @macro "^include_(str|bytes)$")
 (#match? @hit "\\.\\./\\.\\."))
