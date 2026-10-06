; a call inside the body that names the function itself
(function_item name: (identifier) @fn body: (_) @body)
(#has? @body
  (call_expression function: (identifier) @callee
    (#eq? @callee @fn))
  rows: each)
