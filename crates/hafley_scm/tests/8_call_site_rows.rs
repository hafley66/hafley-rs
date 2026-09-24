#![cfg(feature = "rust_syn")]

use hafley_scm::lang::rust::{build_line_starts, call_site_rows};

#[test]
fn sites_const_initializers_and_cfg_classification_share_the_syn_walk() {
    let src = "const TOP: u8 = make();\n#[cfg(test)] fn only() { secret(); }\nfn live() { api::make(); value.run(); S { x: 1 }; E::Variant { x: 1 }; }\nmod inner { static N: u8 = make(); }\n";
    let parsed = syn::parse_file(src).expect("Rust parses");
    let rows = call_site_rows(&parsed, &build_line_starts(src), &[]);
    let sites: Vec<_> = rows
        .sites
        .iter()
        .map(|row| {
            (
                row.callee.as_str(),
                row.callee_path.as_deref(),
                row.cfg.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        sites,
        [
            ("make", None, None),
            ("secret", None, Some("test")),
            ("make", Some("api::make"), None),
            ("run", None, None),
            ("S", None, None),
            ("make", None, None),
        ]
    );
    assert_eq!(
        rows.const_inits
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        ["TOP", "N"]
    );
    assert_eq!(
        rows.test_only_calls,
        [("secret".to_string(), "test".to_string())]
    );
}
