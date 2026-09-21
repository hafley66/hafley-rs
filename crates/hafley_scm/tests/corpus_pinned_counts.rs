//! The corpus: a frozen copy of `crates/sprefa-extract/src/project.rs`, 2799 lines,
//! sha256 d58336b93f2103e321b35750f1ae5940cf6dd5390c75770b1c867c9e1ce3a1f9.
//!
//! Oracle commands run against the live file at that content, in
//! `crates/sprefa-extract`, worktree head aae25cde:
//!
//! ```text
//! ./target/debug/ryi query --lang rust --query '(identifier) @x' src/project.rs | wc -l
//!   -> 3575
//! cargo run --example scm_inside_oracle -- src/project.rs
//!   (function_item) @fnitem
//!   ((identifier) @x (#inside? @x fnitem "end"))      -> 3197
//!   (function_item) @fnitem
//!   ((identifier) @x (#not-inside? @x fnitem "end"))  -> 378
//! ```
//!
//! `ryi query` refuses `#inside?` (`2_source_query.rs` `validate_predicates`), so the
//! two filtered numbers come from the lowering itself through `lower_scm` +
//! `query_ast_rule`; `3197 + 378 == 3575` partitions the unfiltered count.
//! `#has-ancestor? @x "function_item" "end"` is this crate's spelling of the same walk:
//! the kind string replaces the lowering's label indirection.

use hafley_scm::MatchArena;

const CORPUS: &str = include_str!("corpus/sprefa_extract_project.rs.frozen");

const PLAIN: &str = "(identifier) @x";
const INSIDE: &str = "((identifier) @x (#has-ancestor? @x \"function_item\" \"end\"))";
const NOT_INSIDE: &str = "((identifier) @x (#not-has-ancestor? @x \"function_item\" \"end\"))";

#[test]
fn pinned_counts_match_the_current_lowering() {
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("the rust grammar loads");
    let tree = parser.parse(CORPUS, None).expect("the corpus parses");

    let counts: Vec<usize> = [PLAIN, INSIDE, NOT_INSIDE]
        .iter()
        .map(|scm| {
            let q = hafley_scm::build(&language, scm).expect("the query builds");
            let mut arena = MatchArena::default();
            hafley_scm::run(
                &q,
                "sprefa-extract/src/project.rs",
                CORPUS.as_bytes(),
                &tree,
                u32::MAX,
                &mut arena,
            )
            .expect("the run stays under the match limit");
            arena.rows.len()
        })
        .collect();

    assert_eq!(
        counts,
        vec![3575, 3197, 378],
        "{PLAIN} / {INSIDE} / {NOT_INSIDE}"
    );
    assert_eq!(
        counts[1] + counts[2],
        counts[0],
        "the two forms partition the corpus"
    );
}
