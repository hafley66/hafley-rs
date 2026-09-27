; S147: format! builds the SQLite file URI with an unescaped path.
((macro_invocation) @hit
 (#match? @hit "format!.*file:\\{\\}\\?mode=ro"))
