; S038: direct Command::new("git") process launches.
(call_expression
  function: (scoped_identifier) @function
  arguments: (arguments (string_literal) @command)
  (#match? @function "Command::new$")
  (#match? @command "git")
) @hit
