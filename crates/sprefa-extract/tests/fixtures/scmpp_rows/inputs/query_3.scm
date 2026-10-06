(function_item name: (identifier) @fn body: (_) @body)
             (#has? @body ((closure_expression) @closure
               (#has? @closure (call_expression function: (identifier) @callee (#eq? @callee @fn)) rows: each))
               rows: each)