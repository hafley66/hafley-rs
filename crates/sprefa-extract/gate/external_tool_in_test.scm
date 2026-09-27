; ledger: S183 tests spawning Java, Gradle, SCIP Java, CodeQL, gopls, or tsc
(call_expression
  function: (scoped_identifier) @function
  arguments: (arguments (string_literal) @command)
  (#match? @function "Command::new$")
  (#match? @command "(java|gradle|scip-java|codeql|gopls|tsc)")
) @hit
