use boop_store::testing::BoopCommandExt;
use boop_store::Store;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn graph_read_spawns_nothing() {
    let root = std::env::temp_dir().join(format!("boop-graph-read-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let shim_dir = root.join("shims");
    std::fs::create_dir_all(&shim_dir).unwrap();
    let db = root.join("boop.db");
    let _store = Store::open(db.clone()).unwrap();
    let log = root.join("spawn.log");

    for name in ["tmux", "bash", "sh", "ps", "lsof", "pgrep"] {
        let shim = shim_dir.join(name);
        std::fs::write(
            &shim,
            format!("#!/bin/sh\nprintf '%s' '{name}' >> \"$GRAPH_SPAWN_LOG\"\nprintf ' <%s>' \"$@\" >> \"$GRAPH_SPAWN_LOG\"\nprintf '\\n' >> \"$GRAPH_SPAWN_LOG\"\nexit 0\n"),
        )
        .unwrap();
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = std::env::join_paths(std::iter::once(shim_dir.clone()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .unwrap();

    for args in [vec!["db", "agent-summary"], vec!["db", "sessions"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_boop"))
            .args(&args)
            .boop_test_root(&root)
            .env("BOOP_DB", &db)
            .env("GRAPH_SPAWN_LOG", &log)
            .env("PATH", &path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "boop {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(std::fs::read_to_string(&log).unwrap_or_default(), "");
    let _ = std::fs::remove_dir_all(root);
}
