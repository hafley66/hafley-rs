; ledger: S029 std::env::var/var_os/vars/vars_os calls in ryi command and edit functions
(call_expression
  function: (scoped_identifier) @callee
  (#match? @callee "(^|::)env::vars?(_os)?$")
) @hit
