((arguments (integer_literal)? @n) @args
              (#has-parent? @args ((call_expression function: (identifier) @f) (#not-eq? @f @n))))