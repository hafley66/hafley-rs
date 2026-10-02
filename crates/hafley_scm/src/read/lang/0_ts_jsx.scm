; Whole element spans preserve nesting; closing tags emit no second element.
[(jsx_element) (jsx_self_closing_element)] @syntax.jsx.span
[(jsx_opening_element name: (_) @syntax.jsx.name)
 (jsx_self_closing_element name: (_) @syntax.jsx.name)]
(jsx_attribute . (_) @syntax.jsx.attr.name) @syntax.jsx.attr.span
(jsx_attribute . (_) (_) @syntax.jsx.attr.value)
[(jsx_opening_element attribute: (jsx_expression (spread_element)))
 (jsx_self_closing_element attribute: (jsx_expression (spread_element)))] @syntax.jsx.spread.holder
(jsx_expression (spread_element) @syntax.jsx.spread.value) @syntax.jsx.spread.span
