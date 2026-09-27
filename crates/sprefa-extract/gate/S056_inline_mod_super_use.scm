; S056: inline modules containing a use path that starts with super.
(mod_item
  body: (declaration_list
    (use_declaration
      (scoped_identifier (super))
    )
  )
) @hit
