#![cfg(feature = "rust-checker")]
use hafley_scm::read::lang::rust_checker::{module_tree, resolve_path, resolve_written_method};
use std::path::Path;
use std::time::Duration;

#[test]
fn names_answers_follow_workspace_content() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rust_names_cache");
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().canonicalize().unwrap();
    std::fs::copy(fixture.join("Cargo.toml"), root.join("Cargo.toml")).unwrap();
    let source = root.join("0_root.rs");
    let mut rows = Vec::new();
    for (state, file, expected) in [
        ("cold", "0_root.rs", "One"),
        ("warm", "0_root.rs", "One"),
        ("edited", "1_edited.rs", "Two"),
        ("edited warm", "1_edited.rs", "Two"),
    ] {
        let text = std::fs::read_to_string(fixture.join(file)).unwrap();
        std::fs::write(&source, &text).unwrap();
        let host = module_tree(&source, Duration::from_secs(120)).unwrap();
        let offset = text.find(".hit()").unwrap() as u32 + 1;
        let definitions = resolve_written_method(&host, &source, offset, "hit").unwrap();
        let declaration = text.find(&format!("impl {expected}")).unwrap();
        let start = declaration + text[declaration..].find("hit").unwrap();
        let path = resolve_path(&host, &source, &[expected.to_string()]).unwrap();
        rows.push((
            state,
            host.load == Duration::ZERO,
            definitions
                .iter()
                .map(|def| {
                    (
                        def.file == source,
                        def.name.clone(),
                        def.start as usize == start,
                    )
                })
                .collect::<Vec<_>>(),
            path.len(),
        ));
    }
    assert_eq!(
        rows,
        vec![
            ("cold", false, vec![(true, "hit".to_string(), true)], 1),
            ("warm", true, vec![(true, "hit".to_string(), true)], 1),
            ("edited", true, vec![(true, "hit".to_string(), true)], 1),
            ("edited warm", true, vec![(true, "hit".to_string(), true)], 1),
        ]
    );
}
