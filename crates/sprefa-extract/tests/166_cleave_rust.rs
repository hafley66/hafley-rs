#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("cleave_rust_outputs", |case| {
        crate::fixture_runner::commands(case, |step| {
            let args = &step["arguments"];
            serde_json::json!(sprefa_extract::move_cx::walk_corpus(
                std::path::Path::new(args["root"].as_str().unwrap()),
                args["tracked"].as_bool().unwrap(),
            )
            .unwrap())
        })
    });
}

#[test]
fn separate_workspaces_preserve_visibility_and_consumers() {
    use std::path::Path;
    use std::process::{Command, Output};

    fn copy(source: &Path, target: &Path) {
        std::fs::create_dir_all(target).unwrap();
        for entry in std::fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let target = target.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    fn run(root: &Path, state: &Path, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ryii"))
            .arg("cleave")
            .args(arguments)
            .args(["--root", ".", "--json", "--state"])
            .arg(state)
            .current_dir(root)
            .output()
            .unwrap()
    }
    fn passed(output: &Output) -> String {
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        stdout.into_owned()
    }
    fn sources(root: &Path) -> std::collections::BTreeMap<String, String> {
        let mut files = std::collections::BTreeMap::new();
        for package in ["library", "consumer"] {
            let manifest = format!("{package}/Cargo.toml");
            files.insert(
                manifest.clone(),
                std::fs::read_to_string(root.join(manifest)).unwrap(),
            );
            for entry in std::fs::read_dir(root.join(package).join("src")).unwrap() {
                let path = entry.unwrap().path();
                files.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    std::fs::read_to_string(path).unwrap(),
                );
            }
        }
        files
    }
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cleave_rust/separate_workspaces");
    for new_destination in [false, true] {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().join("repo");
        let state = scratch.path().join("state");
        copy(&fixture, &root);
        assert!(Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        if new_destination {
            let manifest = root.join("consumer/Cargo.toml");
            let text = std::fs::read_to_string(&manifest).unwrap();
            std::fs::write(
                &manifest,
                text.replace(
                    "signal-domain = {",
                    "domain = { package = \"signal-domain\",",
                ),
            )
            .unwrap();
            let consumer = root.join("consumer/src/lib.rs");
            let text = std::fs::read_to_string(&consumer).unwrap();
            std::fs::write(&consumer, text.replace("signal_domain::", "domain::")).unwrap();
            std::fs::remove_file(root.join("library/src/_1_destination.rs")).unwrap();
            std::fs::write(
                root.join("library/src/lib.rs"),
                "pub mod _0_source;\npub use _0_source::Signal;\n",
            )
            .unwrap();
        }
        passed(&run(
            &root,
            &state,
            &[
                "library/src/_0_source.rs#Level",
                "library/src/_1_destination.rs",
                "--apply",
            ],
        ));
        let before = sources(&root);
        let dry = passed(&run(
            &root,
            &state,
            &[
                "library/src/_0_source.rs#Signal",
                "library/src/_1_destination.rs",
            ],
        ));
        assert_eq!(sources(&root), before, "dry run changed sources");
        let plan: serde_json::Value =
            serde_json::from_str(dry.lines().find(|line| line.starts_with('{')).unwrap()).unwrap();
        assert_eq!(
            plan["callers"],
            serde_json::json!(["consumer/src/lib.rs", "library/src/lib.rs"])
        );
        assert_eq!(plan["unresolved"], serde_json::json!([]));

        // The same proposed item must still be refused behind a private module.
        let lib = root.join("library/src/lib.rs");
        let public = std::fs::read_to_string(&lib).unwrap();
        std::fs::write(&lib, format!("{public}mod _2_hidden;\n")).unwrap();
        std::fs::write(
            root.join("library/src/_2_hidden.rs"),
            "#[path = \"_2_hidden_child.rs\"] pub mod child;\n",
        )
        .unwrap();
        std::fs::write(root.join("library/src/_2_hidden_child.rs"), "").unwrap();
        let private = sources(&root);
        let refused = run(
            &root,
            &state,
            &[
                "library/src/_0_source.rs#Signal",
                "library/src/_2_hidden_child.rs",
                "--apply",
            ],
        );
        assert_eq!(refused.status.code(), Some(2));
        assert!(
            String::from_utf8_lossy(&refused.stderr)
                .contains("is not visible from consumer/src/lib.rs"),
            "{}",
            String::from_utf8_lossy(&refused.stderr)
        );
        assert_eq!(sources(&root), private, "refused apply changed sources");
        std::fs::write(&lib, public).unwrap();
        std::fs::remove_file(root.join("library/src/_2_hidden.rs")).unwrap();
        std::fs::remove_file(root.join("library/src/_2_hidden_child.rs")).unwrap();

        passed(&run(
            &root,
            &state,
            &[
                "library/src/_0_source.rs#Signal",
                "library/src/_1_destination.rs",
                "--apply",
            ],
        ));
        let after = sources(&root);
        assert!(after["library/src/_1_destination.rs"].contains("impl Signal"));
        assert_eq!(
            after
                .values()
                .map(|text| text.matches("pub enum Signal").count())
                .sum::<usize>(),
            1
        );
        assert_eq!(
            after
                .values()
                .map(|text| text.matches("impl Signal").count())
                .sum::<usize>(),
            1
        );
        assert!(after["library/src/lib.rs"].contains("pub use crate::_1_destination::Signal;"));
        let dependency_name = if new_destination {
            "domain"
        } else {
            "signal_domain"
        };
        assert!(
            after["consumer/src/lib.rs"].contains(&format!("{dependency_name}::_1_destination::"))
        );
        assert!(!after["consumer/src/lib.rs"].contains("_0_source::"));
        let checked = Command::new(env!("CARGO"))
            .args([
                "check",
                "--offline",
                "--quiet",
                "--manifest-path",
                "consumer/Cargo.toml",
            ])
            .env("CARGO_BUILD_JOBS", "1")
            .env("CARGO_TARGET_DIR", scratch.path().join("target"))
            .current_dir(&root)
            .output()
            .unwrap();
        passed(&checked);
    }
}
