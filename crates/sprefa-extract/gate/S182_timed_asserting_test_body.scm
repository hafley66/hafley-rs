; S182: test functions assert over wall-clock measurements, directly or via a wall helper.
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
