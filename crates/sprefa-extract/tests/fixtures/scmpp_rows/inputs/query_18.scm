((function_item name: (identifier) @fn) @f
              (#has? @f (call_expression function: (identifier) @c (#match? @fn "^h")) rows: each))