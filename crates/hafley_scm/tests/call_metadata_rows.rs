//! Snapshots the CallF metadata rows: cfg inheritance, whole-word token
//! matching, owner spans, and the def-restriction on cfg rows.

use std::collections::BTreeSet;

use hafley_scm::build;
use hafley_scm::lang::rust::{
    build_line_starts, call_definition_rows, call_metadata_rows, RUST_CALL_QUERY,
};
use tree_sitter::{Language, Parser};
use tree_sitter_rust::LANGUAGE;

type Cfg = (u32, u32, String);
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

fn snap(src: &str, extra: &[(u32, u32)]) -> (Vec<Cfg>, Vec<Owner>) {
    let parsed = syn::parse_file(src).expect("parses");
    let mut defs = defs_of(src);
    defs.extend(extra.iter().copied());
    let (cfg_rows, owner_rows) = call_metadata_rows(&parsed, &build_line_starts(src), &defs);
    let cfg = cfg_rows
        .iter()
        .map(|row| (row.start, row.end, row.predicate.clone()))
        .collect();
    let owners = owner_rows
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
    (cfg, owners)
}

#[test]
fn nested_cfg_inherits_and_the_outermost_gate_wins() {
    let src =
        "#[cfg(test)]\nmod outer {\n    #[cfg(unix)]\n    fn deep_fn() {\n        1\n    }\n}\n";
    let start = src.find("deep_fn").unwrap() as u32;
    let end = src.find("\n    }\n}").unwrap() as u32 + "\n    }".len() as u32;
    let (cfg, owners) = snap(src, &[]);
    assert_eq!(cfg, vec![(start, end, "test".to_string())]);
    assert!(owners.is_empty());
}

#[test]
fn cfg_predicate_names_test_as_a_whole_word() {
    let src = "#[cfg(any(test, feature = \"x\"))]\nfn gated_fn() {\n    1\n}\n#[cfg(feature = \"testing\")]\nfn other_fn() {\n    2\n}\n";
    let start = src.find("gated_fn").unwrap() as u32;
    let end = src.find("\n}\n#[cfg").unwrap() as u32 + "\n}".len() as u32;
    let (cfg, _) = snap(src, &[]);
    assert_eq!(
        cfg,
        vec![(start, end, "any (test , feature = \"x\")".to_string())]
    );
}

#[test]
fn impl_trait_owners_carry_the_full_trait_path() {
    let src = "struct S;\nmod inner_mod {\n    impl std::fmt::Display for S {\n        fn fmt_fn(&self) -> u32 {\n            1\n        }\n    }\n}\n";
    let start = src.find("fmt_fn").unwrap() as u32;
    let end = src.find("\n        }\n    }\n}").unwrap() as u32 + "\n        }".len() as u32;
    let (_, owners) = snap(src, &[]);
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
    let (_, owners) = snap(src, &[]);
    assert_eq!(
        owners,
        vec![
            (s1, e1, Some("S".to_string()), None),
            (s2, e2, Some("S".to_string()), None)
        ]
    );
}

#[test]
fn impl_cfg_gates_each_method() {
    let src = "struct S;\n#[cfg(test)]\nimpl S {\n    fn gated_m(&self) {\n        1\n    }\n}\n";
    let start = src.find("gated_m").unwrap() as u32;
    let end = src.find("\n    }\n}").unwrap() as u32 + "\n    }".len() as u32;
    let (cfg, owners) = snap(src, &[]);
    assert_eq!(cfg, vec![(start, end, "test".to_string())]);
    assert_eq!(owners.len(), 1, "owner rows are not cfg-gated");
}

#[test]
fn trait_signature_and_default_methods_span_differently() {
    let src =
        "trait T {\n    fn sig_m(&self) -> u32;\n    fn default_m(&self) {\n        1\n    }\n}\n";
    let s1 = src.find("sig_m").unwrap() as u32;
    let e1 = src.find("u32;").unwrap() as u32 + 3;
    let s2 = src.find("default_m").unwrap() as u32;
    let e2 = src.find("\n    }\n}").unwrap() as u32 + "\n    }".len() as u32;
    let (_, owners) = snap(src, &[]);
    assert_eq!(
        owners,
        vec![
            (s1, e1, None, Some("T".to_string())),
            (s2, e2, None, Some("T".to_string()))
        ]
    );
}

#[test]
fn enum_const_and_static_cfg_rows_stay_def_restricted() {
    let src = "#[cfg(test)]\nenum Color {\n    Red,\n    Green,\n}\n#[cfg(test)]\nconst TOP: u32 = 1;\n#[cfg(test)]\nstatic STATE: u8 = 2;\n#[cfg(test)]\nfn orphan_fn() {\n    3\n}\n";
    let red = src.find("Red").unwrap() as u32;
    let green = src.find("Green").unwrap() as u32;
    let top_start = src.find("TOP").unwrap() as u32;
    let top_end = src.find("= 1;").unwrap() as u32 + 3;
    let state_start = src.find("STATE").unwrap() as u32;
    let state_end = src.find("= 2;").unwrap() as u32 + 3;
    let orphan_start = src.find("orphan_fn").unwrap() as u32;
    let orphan_end = src.rfind('}').unwrap() as u32 + 1;
    let (cfg, _) = snap(src, &[]);
    assert_eq!(
        cfg,
        vec![
            (red, red + 3, "test".to_string()),
            (green, green + 5, "test".to_string()),
            (orphan_start, orphan_end, "test".to_string()),
        ],
        "const/static are not engine defs, so they have no cfg row without their ranges"
    );
    let (cfg, _) = snap(src, &[(top_start, top_end), (state_start, state_end)]);
    assert_eq!(
        cfg,
        vec![
            (red, red + 3, "test".to_string()),
            (green, green + 5, "test".to_string()),
            (top_start, top_end, "test".to_string()),
            (state_start, state_end, "test".to_string()),
            (orphan_start, orphan_end, "test".to_string()),
        ],
        "supplying the const-init ranges admits their cfg rows, in walk order"
    );
}
