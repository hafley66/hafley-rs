; ledger: S182 timing assertions using Instant::now or wall helper functions
((source_file
  (attribute_item) @attribute
  .
  (function_item body: (block) @body) @hit
)
 (#match? @attribute "test")
 (#match? @body "Instant::now")
 (#match? @body "assert!"))

((source_file
  (attribute_item) @attribute
  .
  (function_item name: (identifier) @name body: (block) @body) @hit
)
 (#match? @attribute "test")
 (#match? @name "wall")
 (#match? @body "assert!"))
