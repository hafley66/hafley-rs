use tree_sitter::Query;

pub fn intern_names(user: &Query) -> Vec<Box<str>> {
    user.capture_names()
        .iter()
        .map(|name| (*name).into())
        .collect()
}
