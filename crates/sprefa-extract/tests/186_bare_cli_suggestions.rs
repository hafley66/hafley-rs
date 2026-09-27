use std::fs;
use std::process::Command;

#[test]
fn bare_ryii_suggests_ignored_aware_runnable_project_commands() {
    let fixture = tempfile::tempdir().expect("fixture directory");
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::create_dir(fixture.path().join("src")).unwrap();
    fs::write(fixture.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(
        fixture.path().join("package.json"),
        "{\"name\":\"fixture\"}\n",
    )
    .unwrap();
    fs::write(fixture.path().join("index.ts"), "export const value = 1;\n").unwrap();
    fs::write(fixture.path().join(".gitignore"), "ignored/\n").unwrap();
    fs::create_dir(fixture.path().join("ignored")).unwrap();
    fs::write(
        fixture.path().join("ignored/package.json"),
        "{\"name\":\"ignored\"}\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(fixture.path())
        .env("DL_TRAIL", "0")
        .output()
        .expect("run bare ryii");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "fact stream must remain empty");
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 suggestions");
    assert!(stderr.contains("rust Rust workspace ->"), "{stderr}");
    assert!(
        stderr.contains("typescript TypeScript project ->"),
        "{stderr}"
    );
    assert!(
        !stderr.contains("./ignored"),
        "ignored manifest leaked: {stderr}"
    );

    for suggestion in stderr.lines() {
        let Some((_, commands)) = suggestion.split_once(" -> ") else {
            continue;
        };
        for command in commands.split("  |  ") {
            let result = Command::new("sh")
                .arg("-c")
                .arg(command)
                .current_dir(fixture.path())
                .env("DL_TRAIL", "0")
                .output()
                .expect("execute printed command");
            assert!(
                matches!(result.status.code(), Some(0 | 3)),
                "command `{command}` exited {:?}: {}",
                result.status.code(),
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}
