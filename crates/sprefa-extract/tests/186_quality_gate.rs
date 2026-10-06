#![cfg(feature = "cli")]

use std::io::Write;
use std::process::{Command, Stdio};

fn query_rule(rule: &str, source: &std::path::Path) -> String {
    let query = format!("{}/gate/{rule}", env!("CARGO_MANIFEST_DIR"));
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(["query", "--query"])
        .arg(std::fs::read_to_string(query).expect("read rule"))
        .arg(source)
        .output()
        .expect("query binary runs");
    assert!(
        output.status.success(),
        "{rule} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("query output is UTF-8")
}

#[test]
fn query_rule_finds_direct_git_launch_in_fixture() {
    let temp = tempfile::tempdir().expect("temporary fixture directory");
    let source = temp.path().join("fixture.rs");
    std::fs::write(
        &source,
        "fn run() { let _ = std::process::Command::new(\"git\"); }\n",
    )
    .expect("write fixture");

    let stdout = query_rule("git_subprocess.scm", &source);
    assert!(stdout.contains("\"line\":1"), "expected hit, got {stdout}");
    assert!(
        stdout.contains("\"hit\":"),
        "expected captured call, got {stdout}"
    );
}

#[test]
fn generalized_rules_match_different_members_of_each_pattern_class() {
    let temp = tempfile::tempdir().expect("temporary fixture directory");
    let source = temp.path().join("fixture.rs");
    std::fs::write(
        &source,
        r#"
fn inspect(bytes: &[u8], needle: &str, path: &str) {
    let _ = std::env::var("REQUEST_MODE");
    let _ = bytes.windows(needle.len());
    let _ = format!("file:{path}?mode=rw");
    let _ = include_bytes!("../../assets/data.bin");
    let _ = std::process::Command::new("java");
}

#[test]
fn measured_behavior() {
    let started = std::time::Instant::now();
    assert!(started.elapsed() >= std::time::Duration::ZERO);
}
"#,
    )
    .expect("write fixture");

    for rule in [
        "env_read_in_request_path.scm",
        "windows_of_dynamic_len.scm",
        "sqlite_uri_unescaped.scm",
        "include_str_across_crates.scm",
        "wall_clock_assert_in_test.scm",
        "external_tool_in_test.scm",
    ] {
        let output = query_rule(rule, &source);
        assert!(!output.trim().is_empty(), "{rule} did not find its fixture");
    }
}

#[test]
fn sqlite_rule_counts_function_names_across_three_files() {
    let temp = tempfile::tempdir().expect("temporary fixture directory");
    let mut sources = Vec::new();
    for name in ["one.rs", "two.rs", "three.rs"] {
        let source = temp.path().join(name);
        std::fs::write(&source, "fn shared_name() {}\n").expect("write source");
        sources.push(source);
    }
    let database = temp.path().join("facts.db");
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(["fast", "--sqlite"])
        .arg(&database)
        .args(&sources)
        .output()
        .expect("fast sqlite runs");
    assert!(
        output.status.success(),
        "fast stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let rule = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/gate/free_fn_name_in_three_files.sql"
    ))
    .expect("read SQL rule");
    let mut child = Command::new("sqlite3")
        .args(["-tabs", "-noheader"])
        .arg(&database)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("sqlite3 runs");
    child
        .stdin
        .take()
        .expect("sqlite3 stdin is piped")
        .write_all(rule.as_bytes())
        .expect("send SQL rule to sqlite3");
    let output = child.wait_with_output().expect("sqlite3 completes");
    assert!(
        output.status.success(),
        "sqlite3 stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows = String::from_utf8(output.stdout).expect("SQL output is UTF-8");
    assert_eq!(rows.lines().count(), 3, "unexpected rows: {rows}");
    for source in sources {
        assert!(
            rows.contains(&format!("{}\tshared_name\t0", source.display())),
            "missing definition for {} in {rows}",
            source.display()
        );
    }
}

/// The pre-30722bd8 shape: a nested helper inside an if whose expression and
/// statement share a span. The gate's ancestor walk must finish and count it.
#[test]
fn sqlite_rule_terminates_on_equal_span_ancestors() {
    let temp = tempfile::tempdir().unwrap();
    let mut sources = Vec::new();
    for name in ["one.rs", "two.rs", "three.rs"] {
        let source = temp.path().join(name);
        std::fs::write(&source, concat!(
            "fn outer(){if true {fn shared(){}}}",
            "struct S; impl S{fn shared(){}} trait T{fn shared(){}}"
        )).unwrap();
        sources.push(source);
    }
    let database = temp.path().join("facts.db");
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(["fast", "--sqlite"])
        .arg(&database)
        .args(&sources)
        .env("DL_TRAIL", "0")
        .output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let mut child = Command::new("sqlite3")
        .args(["-tabs", "-noheader"])
        .arg(&database)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn().unwrap();
    child.stdin.take().unwrap().write_all(include_bytes!(
        "../gate/free_fn_name_in_three_files.sql"
    )).unwrap();
    let started = std::time::Instant::now();
    while child.try_wait().unwrap().is_none() {
        if started.elapsed() >= std::time::Duration::from_secs(10) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("quality gate ancestor walk exceeded 10s on equal-span nodes");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let rows = String::from_utf8(output.stdout).unwrap()
        .replace(&format!("{}/", temp.path().display()), "");
    assert_eq!(rows, "one.rs\touter\t0\none.rs\tshared\t20\nthree.rs\touter\t0\nthree.rs\tshared\t20\ntwo.rs\touter\t0\ntwo.rs\tshared\t20\n");
}

#[test]
fn repository_quality_gate_matches_its_allowlists() {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/quality-gate.sh");
    let output = Command::new(script)
        .arg(env!("CARGO_BIN_EXE_ryii"))
        .output()
        .expect("quality gate runs");
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
