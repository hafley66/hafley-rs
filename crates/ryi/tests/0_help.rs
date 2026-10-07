use std::process::Command;

#[test]
fn client_help_uses_the_daemon_client_name() {
    let binary = env!("CARGO_BIN_EXE_ryi");
    for args in [&[][..], &["fast"][..], &["ingest"][..]] {
        let output = Command::new(binary)
            .args(args)
            .arg("--help")
            .output()
            .expect("ryi help");
        assert!(output.status.success());
        let help = String::from_utf8(output.stdout).expect("UTF-8 help");
        assert!(help.lines().all(|line| !line.contains("ryii")), "{args:?}: {help}");
        assert!(help.contains("ryi "), "{args:?}: {help}");
        assert!(!help.contains("--daemon-client"), "{args:?}: {help}");
        assert!(!help.contains("  serve   "), "{args:?}: {help}");
    }
}
