//! Generated shell functions preserve noninteractive harness invocations.

use boop_store::testing::BoopCommandExt;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

#[test]
fn generated_wrappers_pass_noninteractive_commands_without_routes() {
    let root = std::env::temp_dir().join(format!("boop-wrapper-pass-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_boop"), root.join("boop")).unwrap();
    for entry in ["codex", "claude", "ccz", "kimi", "opencode"] {
        let path = root.join(entry);
        std::fs::write(&path, "#!/bin/sh\nprintf '%s\\0' \"$@\" > \"$CAPTURE\"\nexit 23\n").unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut paths = vec![root.clone()];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
    let mut receipts = Vec::new();
    for (entry, args) in [
        ("codex", vec!["--help"]),
        ("codex", vec!["-c", "model_reasoning_effort=low", "exec", "a b", ""]),
        ("claude", vec!["-p", "a b"]),
        ("ccz", vec!["--version"]),
        ("kimi", vec!["--prompt", "a b"]),
        ("opencode", vec!["run", "a b"]),
    ] {
        let capture = root.join("args");
        let mut child = Command::new("bash")
            .args(["-c", "eval \"$(boop shell-init bash)\"; \"$@\"", "fixture", entry])
            .args(&args)
            .boop_test_root(&root)
            .env("BOOP_DB", root.join("boop.db"))
            .env("BOOP_MAIL_DIR", root.join("mail"))
            .env("BOOP_NO_SYNC", "1")
            .env("CAPTURE", &capture)
            .env("PATH", std::env::join_paths(&paths).unwrap())
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
            .spawn().unwrap();
        let status = child.wait_timeout(Duration::from_secs(15)).unwrap();
        if status.is_none() { child.kill().unwrap(); child.wait().unwrap(); }
        let captured = std::fs::read(&capture).unwrap_or_default();
        let expected = args.iter().flat_map(|arg| arg.bytes().chain([0])).collect::<Vec<_>>();
        receipts.push((entry, args, status.and_then(|status| status.code()), captured == expected));
    }
    assert!(receipts.iter().all(|(_, _, status, exact)| *status == Some(23) && *exact), "{receipts:?}");
    let store = boop_store::bus::open_store(&root.join("mail")).unwrap();
    assert_eq!(boop_store::bus::read_routes(&root.join("mail")).unwrap().len(), 0);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
