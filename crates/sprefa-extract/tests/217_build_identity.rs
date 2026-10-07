#![cfg(feature = "cli")]

#[test]
fn build_identity_fixture_table() {
    use ryi_proto::build_identity::Build;
    let mut output = String::new();
    for (name, files, enabled) in [
        ("loose", vec![(".git/refs/heads/main", "bbbb\n")], true),
        ("packed", vec![(".git/packed-refs", "# packed\nbbbb refs/heads/main\n")], true),
        ("worktree", vec![(".git", "gitdir: common/worktrees/lane\n"), ("common/worktrees/lane/commondir", "../..\n"), ("common/refs/heads/main", "bbbb\n")], true),
        ("current", vec![(".git/refs/heads/main", "aaaa\n")], true),
        ("unreadable", vec![(".git/config", "")], true),
        ("missing", vec![], true),
        ("unset", vec![(".git/refs/heads/main", "bbbb\n")], false),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        for (path, body) in files {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
        let build = Build { hash: "aaaa", date: "2026-09-27T00:00:00Z", root: &root };
        for binary in ["ryi", "ryii"] {
            output.push_str(&format!("{name} {binary}\n{}", build.warning(binary, enabled).replace(&root.display().to_string(), "<root>")));
        }
        output.push_str(&build.version("ryi").replace(&root.display().to_string(), "<root>"));
    }
    insta::assert_snapshot!("build_identity", output);
}
