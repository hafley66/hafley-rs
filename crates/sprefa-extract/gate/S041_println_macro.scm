; S041: println! calls whose stdout output can be lost in daemon mode.
(macro_invocation
  macro: (identifier) @macro
  (#eq? @macro "println")
) @hit
