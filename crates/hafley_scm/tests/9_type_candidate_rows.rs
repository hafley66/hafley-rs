#![cfg(feature = "rust_syn")]

use hafley_scm::lang::rust::{
    build_line_starts, type_candidate_rows, TypeCandidateKind as Kind, TypeCandidateOwner,
};

#[test]
fn type_candidates_keep_owner_and_reference_order() {
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
            vec![
                ("Clone", Kind::Generic),
                ("Option", Kind::Field),
                ("T", Kind::Field)
            ],
            vec![
                ("Send", Kind::Generic),
                ("Trait", Kind::Impl),
                ("T", Kind::Generic)
            ],
            vec![("S", Kind::Uses), ("Vec", Kind::Uses)],
        ]
    );
}
