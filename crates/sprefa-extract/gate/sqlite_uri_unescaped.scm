; ledger: S147 format! literals containing file: and ?mode=
((macro_invocation
  macro: (identifier) @macro
) @hit
 (#eq? @macro "format")
 (#match? @hit "file:.*\\?mode="))
