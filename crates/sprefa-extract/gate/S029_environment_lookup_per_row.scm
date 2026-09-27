; S029: std::env::var/var_os calls in ryi command and edit functions.
((function_item body: (block) @body) @hit
 (#match? @body "std::env::var(_os)?\\s*\\(")
)
