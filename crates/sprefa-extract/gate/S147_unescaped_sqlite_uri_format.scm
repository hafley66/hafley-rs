; S147: format! literals combine a file: URI with a ?mode= option.
((macro_invocation
  macro: (identifier) @macro
) @hit
 (#eq? @macro "format")
 (#match? @hit "file:.*\\?mode="))
