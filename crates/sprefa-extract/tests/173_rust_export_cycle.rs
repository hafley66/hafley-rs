//! `decode` re-exports through `use super::alloc` while `lib` re-exports
//! `decode`: the resolve recursed until the stack overflowed (issue
//! rust-module-export-cycle, reduced from brotli-decompressor 5.0.3).

use std::path::PathBuf;

#[test]
fn a_reexport_cycle_through_a_use_bound_head_terminates() {
    std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).unwrap();
    let paths: Vec<PathBuf> = ["lib.rs", "decode.rs"]
        .iter()
        .map(|file| PathBuf::from("tests/fixtures/rust_export_cycle/src").join(file))
        .collect();
    let facts = sprefa_extract::diet_scip(&paths).expect("the cycle resolves");
    let imports: Vec<String> = facts
        .iter()
        .filter_map(|fact| match fact {
            sprefa_extract::FlatFact::ResolvedImportRow {
                src_path,
                local,
                target_path,
                kind,
                ..
            } => Some(format!("{src_path} {local} {kind} -> {target_path}")),
            _ => None,
        })
        .collect();
    assert_eq!(
        imports,
        [
            "tests/fixtures/rust_export_cycle/src/lib.rs  module -> tests/fixtures/rust_export_cycle/src/decode.rs",
            "tests/fixtures/rust_export_cycle/src/lib.rs Thing local -> tests/fixtures/rust_export_cycle/src/decode.rs",
        ]
    );
}
