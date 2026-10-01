//! TypeSpec grammar for tree-sitter, vendored from happenslol/tree-sitter-typespec
//! (commit pinned in Cargo.toml) behind the `tree-sitter-language` binding shape.

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_typespec() -> *const ();
}

pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_typespec) };

pub const NODE_TYPES: &str = include_str!("../../src/node-types.json");

#[cfg(test)]
mod tests {
    #[test]
    fn grammar_loads_into_the_runtime() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&super::LANGUAGE.into())
            .expect("typespec grammar loads");
        let tree = parser.parse("model A { x: string; }", None).unwrap();
        assert!(!tree.root_node().has_error());
    }
}
