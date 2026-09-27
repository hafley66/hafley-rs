; S183: tests spawn Java, Gradle, SCIP Java, CodeQL, gopls, or tsc directly.
(call_expression
  function: (scoped_identifier) @function
  arguments: (arguments (string_literal) @command)
  (#match? @function "Command::new$")
  (#match? @command "(java|gradle|scip-java|codeql|gopls|tsc)")
) @hit
