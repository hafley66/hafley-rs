((function_item name: (identifier) @fn body: (_) @body) @item
  (#has? @body (call_expression function: (identifier) @callee (#eq? @callee @fn))))
