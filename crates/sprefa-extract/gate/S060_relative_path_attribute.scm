; S060: #[path = "..."] attributes using a relative path spelling.
(attribute_item
  (attribute
    (identifier) @key
    (string_literal) @value
  )
  (#eq? @key "path")
  (#match? @value "^\\.")
) @hit
