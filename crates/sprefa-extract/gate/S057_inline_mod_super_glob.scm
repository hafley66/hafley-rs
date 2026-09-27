; S057: inline modules containing a super glob import.
(mod_item
  body: (declaration_list
    (use_declaration) @use
  )
  (#match? @use "super::\\*")
) @hit
