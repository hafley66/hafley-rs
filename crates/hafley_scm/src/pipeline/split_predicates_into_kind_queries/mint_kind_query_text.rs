/// `"function_item"` -> `(function_item) @_`
pub fn mint_kind_query_text(kind: &str) -> String {
    format!("({kind}) @_")
}
