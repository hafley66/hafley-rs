((call_expression function: (identifier) @callee) @call
              (#has-ancestor? @call (function_item body: (block (expression_statement (call_expression) @call)))))