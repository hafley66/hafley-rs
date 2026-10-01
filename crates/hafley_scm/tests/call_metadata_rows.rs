//! Snapshots the CallF owner rows: owner spans and trait paths, syn and tree
//! walks in agreement.

use std::collections::BTreeSet;

use hafley_scm::build;
use hafley_scm::lang::rust::{
    build_line_starts, call_definition_rows, call_metadata_rows, call_metadata_rows_from_tree,
    RUST_CALL_QUERY,
};
use tree_sitter::{Language, Parser};
use tree_sitter_rust::LANGUAGE;

type Owner = (u32, u32, Option<String>, Option<String>);

/// The engine's CallF def ranges, exactly what the production adapter feeds.
fn defs_of(src: &str) -> BTreeSet<(u32, u32)> {
    let language = Language::new(LANGUAGE);
    let query = build(&language, RUST_CALL_QUERY).expect("bundled rust call query builds");
    let mut parser = Parser::new();
    parser.set_language(&language).expect("rust grammar");
    let tree = parser.parse(src.as_bytes(), None).expect("rust tree");
    call_definition_rows(&query, "snapshot", src.as_bytes(), &tree)
        .iter()
        .map(|row| (row.range.start, row.range.end))
        .collect()
}

fn snap(src: &str) -> Vec<Owner> {
    let parsed = syn::parse_file(src).expect("parses");
    let defs = defs_of(src);
    let owners: Vec<Owner> = call_metadata_rows(&parsed, &build_line_starts(src))
        .iter()
        .map(|row| {
            (
                row.start,
                row.end,
                row.self_type.clone(),
                row.trait_name.clone(),
            )
        })
        .collect();
    let language = Language::new(LANGUAGE);
    let mut parser = Parser::new();
    parser.set_language(&language).expect("rust grammar");
    let tree = parser.parse(src.as_bytes(), None).expect("rust tree");
    let tree_owners = call_metadata_rows_from_tree(
        &tree,
        src.as_bytes(),
        &defs.iter().copied().collect::<Vec<_>>(),
    );
    assert_eq!(
        owners,
        tree_owners
            .iter()
            .map(|row| (
                row.start,
                row.end,
                row.self_type.clone(),
                row.trait_name.clone(),
            ))
            .collect::<Vec<_>>()
    );
    owners
}

#[test]
fn impl_trait_owners_carry_the_full_trait_path() {
    let src = "struct S;\nmod inner_mod {\n    impl std::fmt::Display for S {\n        fn fmt_fn(&self) -> u32 {\n            1\n        }\n    }\n}\n";
    let start = src.find("fmt_fn").unwrap() as u32;
    let end = src.find("\n        }\n    }\n}").unwrap() as u32 + "\n        }".len() as u32;
    let owners = snap(src);
    assert_eq!(
        owners,
        vec![(
            start,
            end,
            Some("S".to_string()),
            Some("std::fmt::Display".to_string())
        )]
    );
}

#[test]
fn inherent_impl_owners_have_no_trait() {
    let src = "struct S;\nimpl S {\n    fn m_one(&self) {\n        1\n    }\n    fn m_two(self) {\n        2\n    }\n}\n";
    let s1 = src.find("m_one").unwrap() as u32;
    let e1 = src.find("\n    }\n    fn").unwrap() as u32 + "\n    }".len() as u32;
    let s2 = src.find("m_two").unwrap() as u32;
    let e2 = src.find("\n    }\n}").unwrap() as u32 + "\n    }".len() as u32;
    let owners = snap(src);
    assert_eq!(
        owners,
        vec![
            (s1, e1, Some("S".to_string()), None),
            (s2, e2, Some("S".to_string()), None)
        ]
    );
}
#[test]
fn trait_signature_and_default_methods_span_differently() {
    let src =
        "trait T {\n    fn sig_m(&self) -> u32;\n    fn default_m(&self) {\n        1\n    }\n}\n";
    let s1 = src.find("sig_m").unwrap() as u32;
    let e1 = src.find("u32;").unwrap() as u32 + 3;
    let s2 = src.find("default_m").unwrap() as u32;
    let e2 = src.find("\n    }\n}").unwrap() as u32 + "\n    }".len() as u32;
    let owners = snap(src);
    assert_eq!(
        owners,
        vec![
            (s1, e1, None, Some("T".to_string())),
            (s2, e2, None, Some("T".to_string()))
        ]
    );
}
