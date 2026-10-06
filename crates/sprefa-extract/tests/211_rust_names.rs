#![cfg(feature = "cli")]
#[path = "support/0_dogfood_fixture.rs"]
mod support;
use support::Fixture;

#[test]
fn shared_names_provider_routes_graph_and_edits() {
    let mut results = Vec::new();
    for case in ["graph", "rename", "move", "orphan", "method"] {
        let fixture = Fixture::from_dir("rust_names_resolution");
        let args: &[&str] = match case {
            "graph" => &[
                "--resolve",
                "--arms",
                "call",
                "0_root.rs",
                "1_shared.rs",
                "2_bin.rs",
            ],
            "rename" => &["rename", "1_shared.rs#target", "changed", "--commit"],
            "move" => &["move", "1_shared.rs", "4_shared.rs", "--commit"],
            "orphan" => &["rename", "3_orphan.rs#lost", "changed", "--commit"],
            "method" => &["rename", "1_shared.rs#method", "changed", "--commit"],
            _ => unreachable!(),
        };
        let output = if case == "graph" {
            std::process::Command::new(env!("CARGO_BIN_EXE_ryii"))
                .current_dir(&fixture.root)
                .args(args)
                .args(["--root", fixture.root.to_str().unwrap()])
                .env("KACHE_DISABLED", "1")
                .output()
                .unwrap()
        } else {
            fixture.run(&fixture.root, args)
        };
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            matches!(case, "orphan" | "method") || output.status.success(),
            "{case}: {stderr}"
        );
        let detail = match case {
            "graph" => {
                let mut rows = stdout
                    .lines()
                    .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
                    .filter_map(|row| match row["record"].as_str()? {
                        "resolved_edge" => Some(format!(
                            "{} -> {}#{}",
                            row["caller_name"].as_str()?,
                            row["callee_path"].as_str()?.rsplit('/').next()?,
                            row["callee_name"].as_str()?
                        )),
                        "unresolved" if row["reason"] == "needs_types" => {
                            Some("method: needs_types".into())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                rows.sort();
                rows.join("\n")
            }
            "rename" => ["0_root.rs", "1_shared.rs", "2_bin.rs"]
                .into_iter()
                .map(|file| format!("{file}: {}", fixture.read(file)))
                .collect::<Vec<_>>()
                .join("\n"),
            "move" => ["0_root.rs", "2_bin.rs", "4_shared.rs"]
                .into_iter()
                .map(|file| format!("{file}: {}", fixture.read(file)))
                .collect::<Vec<_>>()
                .join("\n"),
            "method" => format!("needs_types: {}", stderr.contains("needs_types")),
            "orphan" => format!(
                "outside_workspace: {}",
                stderr.contains("outside_workspace")
            ),
            _ => unreachable!(),
        };
        results.push(format!("{case}: {}\n{detail}", output.status.success()));
    }
    insta::assert_snapshot!(results.join("\n\n"), @r#"
graph: true
main -> 1_shared.rs#target
main -> 1_shared.rs#target
method: needs_types
run -> 1_shared.rs#target
run -> 1_shared.rs#target

rename: true
0_root.rs: #[path = "1_shared.rs"]
pub mod shared;
pub use shared::changed as renamed;
pub use shared::*;

pub mod nested {
    use crate::shared::changed as local;
    pub fn run() { local(); super::shared::changed(); }
}

pub fn method_user(item: shared::Item) { item.method(); }

1_shared.rs: pub fn changed() {}
pub struct Item;
impl Item { pub fn method(&self) {} }

2_bin.rs: #[path = "1_shared.rs"]
mod included;
fn main() { names_fixture::renamed(); crate::included::changed(); }


move: true
0_root.rs: #[path = "4_shared.rs"]
pub mod shared;
pub use shared::target as renamed;
pub use shared::*;

pub mod nested {
    use crate::shared::target as local;
    pub fn run() { local(); super::shared::target(); }
}

pub fn method_user(item: shared::Item) { item.method(); }

2_bin.rs: #[path = "4_shared.rs"]
mod included;
fn main() { names_fixture::renamed(); crate::included::target(); }

4_shared.rs: pub fn target() {}
pub struct Item;
impl Item { pub fn method(&self) {} }


orphan: false
outside_workspace: true

method: false
needs_types: true
"#);
}
