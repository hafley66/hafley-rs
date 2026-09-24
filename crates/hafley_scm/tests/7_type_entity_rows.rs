#![cfg(feature = "rust_syn")]

use hafley_scm::lang::rust::{build_line_starts, type_entity_rows};

#[test]
fn entity_walk_collects_bare_impl_heads_without_a_second_syntax_walk() {
    let src = "struct Root;\nimpl Root {}\nmod inner { struct Child; impl &Child {} }\nimpl outer::Other {}\n";
    let parsed = syn::parse_file(src).expect("Rust parses");
    let rows = type_entity_rows(&parsed, &build_line_starts(src));
    let heads: Vec<(&str, &str)> = rows
        .impl_self_heads
        .iter()
        .map(|row| {
            (
                row.name.as_str(),
                &src[row.range.start as usize..row.range.end as usize],
            )
        })
        .collect();
    assert_eq!(heads, [("Root", "Root"), ("Child", "Child")]);
    assert_eq!(
        rows.entities.iter().map(|row| row.name.as_str()).collect::<Vec<_>>(),
        ["Root", "Child"]
    );
}
