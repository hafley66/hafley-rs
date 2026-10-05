; All rule decisions live in 3_violations.sql. The ancestor relation requests
; the generic CST store used there for function boundaries and branch fields.
((call_expression
   function: [(identifier) @callee
              (member_expression property: (property_identifier) @callee)] @invoked) @hook
 (#match? @callee "^use[A-Z0-9]")
 (#has-ancestor? @hook program))
