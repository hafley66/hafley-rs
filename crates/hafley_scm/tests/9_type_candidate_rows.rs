#![cfg(feature = "rust_syn")]

use hafley_scm::lang::rust::{
    build_line_starts, type_candidate_rows, type_candidate_rows_from_tree,
    TypeCandidateKind as Kind, TypeCandidateOwner,
};

#[test]
fn type_candidates_drop_generic_parameters_and_keep_owner_reference_order() {
    let src = "struct S<T: Clone> { x: Option<T> }\nimpl<T: Send> Trait<u8> for S<T> {}\ntype Alias = Vec<S<i32>>;\n";
    let parsed = syn::parse_file(src).expect("Rust parses");
    let groups = type_candidate_rows(&parsed, &build_line_starts(src));
    assert_eq!(groups.len(), 3);
    assert!(matches!(groups[0].owner, TypeCandidateOwner::Declared(_)));
    assert!(
        matches!(&groups[1].owner, TypeCandidateOwner::Impl { primary_name, bare_head: Some(_) } if primary_name == "S")
    );
    assert!(matches!(groups[2].owner, TypeCandidateOwner::Declared(_)));
    let receipt: Vec<_> = groups
        .iter()
        .map(|group| {
            group
                .candidates
                .iter()
                .map(|row| (row.to.as_str(), row.kind))
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(
        receipt,
        [
            vec![("Clone", Kind::Generic), ("Option", Kind::Field)],
            vec![("Send", Kind::Generic), ("Trait", Kind::Impl)],
            vec![("S", Kind::Uses), ("Vec", Kind::Uses)],
        ]
    );
}

#[test]
fn tree_type_candidates_match_the_shared_rust_fixture_projection() {
    let src = "struct S<T: Clone> { x: Option<T> }\nimpl<T: Send> Trait<u8> for S<T> {}\ntype Alias = Vec<S<i32>>;\n";
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("Rust grammar loads");
    let tree = parser.parse(src, None).expect("Rust tree parses");
    let groups = type_candidate_rows_from_tree(&tree, src.as_bytes());
    assert_eq!(groups.len(), 3);
    assert!(matches!(groups[0].owner, TypeCandidateOwner::Declared(_)));
    assert!(matches!(
        &groups[1].owner,
        TypeCandidateOwner::Impl { primary_name, bare_head: Some(_) } if primary_name == "S"
    ));
    assert!(matches!(groups[2].owner, TypeCandidateOwner::Declared(_)));
    let receipt: Vec<_> = groups
        .iter()
        .map(|group| {
            group
                .candidates
                .iter()
                .map(|row| (row.to.as_str(), row.kind))
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(
        receipt,
        [
            vec![("Clone", Kind::Generic), ("Option", Kind::Field)],
            vec![("Send", Kind::Generic), ("Trait", Kind::Impl)],
            vec![("S", Kind::Uses), ("Vec", Kind::Uses)],
        ]
    );
}

#[test]
fn tree_type_candidates_match_syn_across_the_pinned_soopy_fixture() {
    let fixture_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let roots = [
        fixture_root.join("../sprefa-extract/tests/fixtures/ratchet_soopy/src"),
        fixture_root.join("../sprefa-extract/tests/fixtures/rust_visibility/app/src"),
        fixture_root.join("../sprefa-extract/tests/fixtures/type_ladder_scope/src"),
    ];
    let mut pending = roots.to_vec();
    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(&path).expect("fixture directory reads") {
            let entry = entry.expect("fixture entry reads");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(std::ffi::OsStr::to_str) != Some("rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("Rust fixture reads as UTF-8");
            let syn_file = syn::parse_file(&source).expect("Syn parses the Rust fixture");
            let syn_groups = type_candidate_rows(&syn_file, &build_line_starts(&source));
            let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
            let mut parser = tree_sitter::Parser::new();
            parser.set_language(&language).expect("Rust grammar loads");
            let tree = parser
                .parse(&source, None)
                .expect("tree-sitter parses fixture");
            let tree_groups = type_candidate_rows_from_tree(&tree, source.as_bytes());
            if tree_groups != syn_groups {
                let examples = tree_groups
                    .iter()
                    .zip(&syn_groups)
                    .enumerate()
                    .filter(|(_, (tree, syn))| tree != syn)
                    .take(8)
                    .map(|(index, (tree, syn))| {
                        (
                            index,
                            &tree.owner,
                            &tree.candidates,
                            &syn.owner,
                            &syn.candidates,
                        )
                    })
                    .collect::<Vec<_>>();
                panic!(
                    "candidate rows diverge for {}: tree groups={}, syn groups={}, examples={examples:#?}",
                    path.display(),
                    tree_groups.len(),
                    syn_groups.len(),
                );
            }
        }
    }
}
