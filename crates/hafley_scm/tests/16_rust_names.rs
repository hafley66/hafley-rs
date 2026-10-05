#![cfg(feature = "rust-checker")]
use hafley_scm::read::lang::rust_checker::{
    module_places, module_tree, resolve_method, resolve_path, Abstain,
};
use hafley_scm::read::lang::rust_checker::{resolve_prefix, CheckerError};
use std::path::Path;
use std::time::Duration;

#[test]
fn names_provider_lifecycle() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().canonicalize().unwrap();
    assert!(matches!(
        module_tree(&root.join("0_root.rs"), Duration::from_secs(120)),
        Err(CheckerError::NoWorkspace(_))
    ));
    for file in [
        "Cargo.toml",
        "0_root.rs",
        "1_shared.rs",
        "2_bin.rs",
        "3_build.rs",
    ] {
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/rust_names")
                .join(file),
            root.join(file),
        )
        .unwrap();
    }
    let source = root.join("1_shared.rs");
    let budget = Duration::from_secs(120);
    let cold = module_tree(&source, budget).unwrap();
    let warm = module_tree(&source, budget).unwrap();
    assert_eq!(cold.key, warm.key);
    assert_eq!(warm.load, Duration::ZERO);
    assert_eq!(
        module_places(&cold, &root.join("3_build.rs")).unwrap()[0].path,
        Vec::<String>::new()
    );
    let mut output = Vec::new();
    for (file, path) in [
        ("0_root.rs", "renamed"),
        ("0_root.rs", "target"),
        ("1_shared.rs", "crate::shared::target"),
        ("2_bin.rs", "names_fixture::renamed"),
        ("2_bin.rs", "crate::included::target"),
    ] {
        let names = path.split("::").map(str::to_string).collect::<Vec<_>>();
        let definitions = resolve_path(&cold, &root.join(file), &names).unwrap();
        output.push((
            file,
            path,
            definitions
                .into_iter()
                .map(|definition| {
                    (
                        definition
                            .file
                            .strip_prefix(&root)
                            .unwrap()
                            .to_string_lossy()
                            .to_string(),
                        definition.name,
                    )
                })
                .collect::<Vec<_>>(),
        ));
    }
    assert_eq!(
        output,
        vec![
            (
                "0_root.rs",
                "renamed",
                vec![("1_shared.rs".into(), "target".into())]
            ),
            (
                "0_root.rs",
                "target",
                vec![("1_shared.rs".into(), "target".into())]
            ),
            (
                "1_shared.rs",
                "crate::shared::target",
                vec![("1_shared.rs".into(), "target".into())]
            ),
            (
                "2_bin.rs",
                "names_fixture::renamed",
                vec![("1_shared.rs".into(), "target".into())]
            ),
            (
                "2_bin.rs",
                "crate::included::target",
                vec![("1_shared.rs".into(), "target".into())]
            ),
        ]
    );
    assert_eq!(
        resolve_method(&cold, &source, "method"),
        Err(Abstain::NeedsTypes)
    );
    let prefix_rows = module_places(&cold, &source)
        .unwrap()
        .into_iter()
        .flat_map(|place| {
            let host = &cold;
            ["self", "super", "super::super"]
                .into_iter()
                .map(move |prefix| {
                    let segments = prefix.split("::").map(str::to_string).collect::<Vec<_>>();
                    let result = resolve_prefix(host, &place, &[], &segments).map(|places| {
                        places
                            .into_iter()
                            .map(|place| place.path)
                            .collect::<Vec<_>>()
                    });
                    (place.target, prefix, result)
                })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        prefix_rows,
        vec![
            (0, "self", Ok(vec![vec!["shared".into()]])),
            (0, "super", Ok(vec![vec![]])),
            (0, "super::super", Err(Abstain::UnresolvedPath)),
            (1, "self", Ok(vec![vec!["included".into()]])),
            (1, "super", Ok(vec![vec![]])),
            (1, "super::super", Err(Abstain::UnresolvedPath)),
        ]
    );
    let places = module_places(&cold, &source)
        .unwrap()
        .into_iter()
        .map(|place| {
            (
                place
                    .crate_root
                    .strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
                place.path,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        places,
        vec![
            ("0_root.rs".into(), vec!["shared".into()]),
            ("2_bin.rs".into(), vec!["included".into()])
        ]
    );
    std::fs::write(root.join("3_orphan.rs"), "pub fn orphan() {}\n").unwrap();
    assert!(matches!(
        module_places(&warm, &root.join("3_orphan.rs")),
        Err(Abstain::OutsideWorkspace)
    ));
    std::fs::write(root.join("1_shared.rs"), "pub fn changed() {}\n").unwrap();
    let edited = module_tree(&source, budget).unwrap();
    assert_eq!(edited.key, cold.key);
    assert_eq!(edited.load, Duration::ZERO);
    assert_eq!(
        resolve_path(&edited, &source, &["changed".into()]).unwrap()[0].name,
        "changed"
    );
    edited
        .sync(&[(source.clone(), "pub fn overlay() {}\n".into())])
        .unwrap();
    assert_eq!(
        resolve_path(&edited, &source, &["overlay".into()]).unwrap()[0].name,
        "overlay"
    );
    drop(edited);
    assert_eq!(
        resolve_path(&cold, &source, &["changed".into()]).unwrap()[0].name,
        "changed"
    );
    std::fs::write(root.join("3_created.rs"), "pub fn created() {}\n").unwrap();
    std::fs::write(
        root.join("0_root.rs"),
        "#[path=\"3_created.rs\"] pub mod child;\n",
    )
    .unwrap();
    let created = module_tree(&root.join("3_created.rs"), budget).unwrap();
    assert_eq!(created.key, cold.key);
    assert_eq!(created.load, Duration::ZERO);
    assert_eq!(
        resolve_path(
            &created,
            &root.join("0_root.rs"),
            &["child".into(), "created".into()]
        )
        .unwrap()[0]
            .name,
        "created"
    );
    std::fs::write(root.join("Cargo.lock"), "# content key probe\n").unwrap();
    let relocked = module_tree(&source, budget).unwrap();
    assert_ne!(relocked.key, cold.key);
    assert!(relocked.load > Duration::ZERO);
    let manifest = root.join("Cargo.toml");
    std::fs::write(
        &manifest,
        std::fs::read_to_string(&manifest)
            .unwrap()
            .replace("names-fixture", "names-changed"),
    )
    .unwrap();
    let renamed = module_tree(&source, budget).unwrap();
    assert_ne!(renamed.key, relocked.key);
    assert_eq!(
        module_places(&renamed, &root.join("0_root.rs")).unwrap()[0].crate_name,
        "names_changed"
    );
}
