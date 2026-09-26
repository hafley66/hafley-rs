use std::process::Command;

fn output(program: &str, args: &[&str]) -> String {
    Command::new(program).args(args).output().ok()
        .filter(|result| result.status.success())
        .map(|result| String::from_utf8_lossy(&result.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn main() {
    for git_path in ["HEAD", "index", "packed-refs"] {
        let path = output("git", &["rev-parse", "--git-path", git_path]);
        if path != "unknown" { println!("cargo:rerun-if-changed={path}"); }
    }
    let reference = output("git", &["symbolic-ref", "-q", "HEAD"]);
    if reference != "unknown" {
        let path = output("git", &["rev-parse", "--git-path", &reference]);
        if path != "unknown" { println!("cargo:rerun-if-changed={path}"); }
    }
    let git_hash = output("git", &["rev-parse", "--short=12", "HEAD"]);
    println!("cargo:rustc-env=SPREFA_BUILD_GIT_HASH={git_hash}");
    println!("cargo:rustc-env=SPREFA_BUILD_DATETIME={}", output("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"]));
}
