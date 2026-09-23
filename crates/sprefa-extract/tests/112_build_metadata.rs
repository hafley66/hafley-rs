use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

fn run(dir: &Path, program: &str, args: &[&str]) -> Output {
    Command::new(program)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|error| panic!("run {program}: {error}"))
}

fn must_run(dir: &Path, program: &str, args: &[&str]) -> Output {
    let output = run(dir, program, args);
    assert!(
        output.status.success(),
        "{program} {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn commit(dir: &Path, message: &str) {
    must_run(dir, "git", &["add", "."]);
    must_run(
        dir,
        "git",
        &[
            "-c",
            "user.name=sprefa-test",
            "-c",
            "user.email=sprefa-test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            message,
        ],
    );
}

fn metadata(dir: &Path) -> (String, String) {
    let output = Command::new("cargo")
        .args(["run", "--quiet"])
        .current_dir(dir)
        .env_remove("CARGO_TARGET_DIR")
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .output()
        .expect("run isolated cargo");
    assert!(
        output.status.success(),
        "cargo run: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let line = String::from_utf8(output.stdout).expect("metadata is UTF-8");
    let (hash, datetime) = line.trim().split_once(' ').expect("hash and datetime");
    (hash.to_string(), datetime.to_string())
}

#[test]
fn build_metadata_tracks_source_and_checked_out_branch() {
    let root = std::env::temp_dir().join(format!(
        "sprefa-build-metadata-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).expect("miniature crate");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"build-metadata-probe\"\nversion = \"0.0.0\"\nedition = \"2021\"\nbuild = \"build.rs\"\n",
    )
    .expect("manifest");
    std::fs::write(
        root.join("build.rs"),
        format!("{}\nfn main() {{ run(); }}\n", include_str!("../build/0_metadata.rs")),
    )
    .expect("build script");
    std::fs::write(
        root.join("src/main.rs"),
        "fn main() { println!(\"{} {}\", env!(\"SPREFA_BUILD_GIT_HASH\"), env!(\"SPREFA_BUILD_DATETIME\")); }\n",
    )
    .expect("main");
    must_run(&root, "git", &["init", "--quiet"]);
    commit(&root, "initial");

    let (initial_hash, initial_datetime) = metadata(&root);
    std::thread::sleep(Duration::from_secs(1));
    std::fs::write(
        root.join("src/main.rs"),
        "// source changed\nfn main() { println!(\"{} {}\", env!(\"SPREFA_BUILD_GIT_HASH\"), env!(\"SPREFA_BUILD_DATETIME\")); }\n",
    )
    .expect("changed main");
    let (source_hash, source_datetime) = metadata(&root);
    assert_eq!(source_hash, initial_hash);
    assert_ne!(source_datetime, initial_datetime);

    commit(&root, "source changed");
    let (committed_hash, _) = metadata(&root);
    assert_ne!(committed_hash, source_hash);
}

/// The root manifest's `workspace.exclude` entries, as written.
fn root_excludes(root_manifest: &str) -> Vec<String> {
    let start = root_manifest
        .find("\nexclude = [")
        .expect("root Cargo.toml declares workspace.exclude");
    let list = &root_manifest[start..];
    let end = list.find(']').expect("exclude list closes");
    list[..end]
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

/// `path = "..."` targets of a manifest: cargo reads those manifests during
/// workspace discovery even under `--no-deps`, so the probe stubs each one.
fn path_dependencies(crate_dir: &Path) -> Vec<String> {
    let manifest = std::fs::read_to_string(crate_dir.join("Cargo.toml")).expect("crate manifest");
    manifest
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split_once("path = \"").map(|(_, rest)| rest))
        .filter_map(|rest| rest.split('"').next())
        .filter(|path| path.starts_with(".."))
        .map(str::to_string)
        .collect()
}

/// Every crate the root workspace excludes must declare `[workspace]`, or cargo
/// walks up from a git worktree under `.claude/worktrees/<name>/` past the
/// worktree root (whose `exclude` matches) into the parent checkout (whose
/// `exclude` does not) and refuses to resolve. Proven by `cargo metadata` from
/// a nested copy shaped like that worktree, not by grepping the manifest.
#[test]
fn excluded_crates_resolve_from_a_nested_worktree() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root_manifest = std::fs::read_to_string(repo.join("Cargo.toml")).expect("root manifest");
    let excludes = root_excludes(&root_manifest);
    assert!(
        excludes.iter().any(|p| p == "crates/sprefa-extract"),
        "exclude list parsed: {excludes:?}"
    );

    let tmp = std::env::temp_dir().join(format!("sprefa-nested-worktree-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let worktree = tmp.join(".claude/worktrees/probe");
    for dir in [&tmp, &worktree] {
        std::fs::create_dir_all(dir).expect("workspace dir");
        std::fs::write(dir.join("Cargo.toml"), &root_manifest).expect("workspace manifest");
    }

    for excluded in &excludes {
        let crate_dir = worktree.join(excluded);
        std::fs::create_dir_all(crate_dir.join("src/bin")).expect("crate dir");
        std::fs::copy(repo.join(excluded).join("Cargo.toml"), crate_dir.join("Cargo.toml"))
            .expect("crate manifest");
        for stub in ["src/lib.rs", "src/main.rs", "src/bin/extract.rs", "build.rs"] {
            std::fs::write(crate_dir.join(stub), "fn main() {}\n").expect("target stub");
        }
        for sibling in path_dependencies(&crate_dir) {
            let dep_dir = crate_dir.join(&sibling);
            if dep_dir.join("Cargo.toml").exists() {
                continue;
            }
            std::fs::create_dir_all(dep_dir.join("src")).expect("sibling dir");
            let name = Path::new(&sibling).file_name().unwrap().to_string_lossy();
            std::fs::write(
                dep_dir.join("Cargo.toml"),
                format!("[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n"),
            )
            .expect("sibling manifest");
            std::fs::write(dep_dir.join("src/lib.rs"), "").expect("sibling lib");
        }
        let output = Command::new("cargo")
            .args(["metadata", "--no-deps", "--offline", "--format-version", "1"])
            .current_dir(&crate_dir)
            .output()
            .expect("run cargo metadata");
        assert!(
            output.status.success(),
            "{excluded}: add `[workspace]` to its Cargo.toml\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        let expected_root = crate_dir.canonicalize().expect("crate dir exists");
        assert!(
            stdout.contains(&format!("\"workspace_root\":\"{}\"", expected_root.display())),
            "{excluded}: workspace_root is not the crate itself"
        );
    }
    let _ = std::fs::remove_dir_all(&tmp);
}
