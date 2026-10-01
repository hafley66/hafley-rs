#![cfg(feature = "rust_syn")]

use hafley_scm::lang::rust::{
    type_entity_rows, type_entity_rows_from_tree, SignatureSlot, TypeEntityKind,
};

#[test]
fn entity_walk_collects_bare_impl_heads_without_a_second_syntax_walk() {
    let src = "struct Root;\nimpl Root {}\nmod inner { struct Child; impl &Child {} }\nimpl outer::Other {}\n";
    let parsed = hafley_scm::lang::rust::parse_rust_file(src).expect("Rust parses");
    let rows = type_entity_rows(&parsed);
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
        rows.entities
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        ["Root", "Child"]
    );
}

#[test]
fn entity_walk_captures_doc_sections_and_impl_parent() {
    let src = "/// Root\n/// # Safety\n/// Check it.\nstruct Root;\nimpl Root { /// Method\n    fn run(&self) {} }\n/// Ignored\nconst VALUE: u8 = 1;\n";
    let parsed = hafley_scm::lang::rust::parse_rust_file(src).expect("Rust parses");
    let rows = type_entity_rows(&parsed);
    let receipt: Vec<_> = rows
        .docs
        .iter()
        .map(|row| {
            (
                &src[row.range.start as usize..row.range.end as usize],
                row.parent.as_deref(),
                row.text.as_str(),
                row.sections
                    .iter()
                    .map(|section| (section.heading.as_str(), section.body.as_str()))
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    assert_eq!(
        receipt,
        [
            (
                "Root",
                None,
                "Root\n# Safety\nCheck it.",
                vec![("Safety", "Check it.")]
            ),
            ("run", Some("Root"), "Method", vec![]),
        ]
    );
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("Rust grammar loads");
    let tree = parser.parse(src, None).expect("Rust tree parses");
    let tree_rows = type_entity_rows_from_tree(&tree, src.as_bytes());
    let tree_receipt: Vec<_> = tree_rows
        .docs
        .iter()
        .map(|row| {
            (
                &src[row.range.start as usize..row.range.end as usize],
                row.parent.as_deref(),
                row.text.as_str(),
                row.sections
                    .iter()
                    .map(|section| (section.heading.as_str(), section.body.as_str()))
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    assert_eq!(tree_receipt, receipt);
}

#[test]
fn tree_entity_walk_projects_the_shared_tree_and_signature_types() {
    let src = "struct Item<T> { value: Vec<T> }\ntrait Read { type Out; fn read(&self) -> Self::Out; fn ready(&self) {} }\nimpl Item<u8> { fn get(&self, index: usize) -> Option<u8> { None } }\nimpl Plain {}\nfn make(input: Item<u8>) -> Vec<u8> { todo!() }\n";
    let parsed = hafley_scm::lang::rust::parse_rust_file(src).expect("Rust parses");
    let syn_rows = type_entity_rows(&parsed);
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("Rust grammar loads");
    let tree = parser.parse(src, None).expect("Rust tree parses");
    let rows = type_entity_rows_from_tree(&tree, src.as_bytes());
    let receipt = rows
        .entities
        .iter()
        .map(|row| {
            (
                row.name.as_str(),
                row.kind,
                row.sigs
                    .iter()
                    .map(|sig| (sig.slot, sig.pos, sig.name.as_str()))
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        receipt,
        [
            ("Item", TypeEntityKind::Struct, vec![]),
            ("Read", TypeEntityKind::Trait, vec![]),
            ("Out", TypeEntityKind::Alias, vec![]),
            ("ready", TypeEntityKind::Method, vec![]),
            (
                "get",
                TypeEntityKind::Method,
                vec![(SignatureSlot::Ret, 0, "Option")]
            ),
            (
                "make",
                TypeEntityKind::Function,
                vec![
                    (SignatureSlot::Param, 0, "Item"),
                    (SignatureSlot::Ret, 0, "Vec")
                ]
            ),
        ]
    );
    assert_eq!(
        rows.impl_self_heads
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        ["Item", "Plain"]
    );
    assert_eq!(
        rows.impl_self_heads, syn_rows.impl_self_heads,
        "tree impl self heads must preserve Syn's output"
    );
}
