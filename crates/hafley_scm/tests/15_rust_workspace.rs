#![cfg(feature = "cargo-metadata")]

#[test]
fn nearest_manifest_selects_cargo_workspace() {
    let fixture = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(fixture.path().join("member/src")).unwrap();
    std::fs::write(fixture.path().join("Cargo.toml"), "[workspace]\nmembers=['member']\nresolver='2'\n").unwrap();
    std::fs::write(fixture.path().join("member/Cargo.toml"), "[package]\nname='member'\nversion='0.1.0'\nedition='2021'\n").unwrap();
    std::fs::write(fixture.path().join("member/src/lib.rs"), "").unwrap();
    for source in ["member/src/lib.rs", "member/src/unborn.rs"] {
        let workspace = hafley_scm::read::lang::rust_workspace::discover(&fixture.path().join(source)).unwrap();
        assert_eq!((workspace.manifest.strip_prefix(fixture.path().canonicalize().unwrap()).unwrap().to_string_lossy().to_string(), workspace.metadata.workspace_packages().iter().map(|package| package.name.to_string()).collect::<Vec<_>>()), ("member/Cargo.toml".to_string(), vec!["member".to_string()]));
    }
    assert!(hafley_scm::read::lang::rust_workspace::discover(&fixture.path().join("orphan.rs")).is_ok());
}
