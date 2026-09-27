use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Fixture {
    root: PathBuf,
}

fn fixture(label: &str) -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "ryi_stratify_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stratify_ts");
    copy_tree(&source, &root);
    Fixture {
        root: root.canonicalize().unwrap(),
    }
}

fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let to = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), to).unwrap();
        }
    }
}

fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    std::fs::read_dir(root)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read(path).unwrap(),
            )
        })
        .collect()
}

fn run(fixture: &Fixture, extra: &[&str]) -> std::process::Output {
    run_froms(fixture, &["main.ts:main=1"], extra)
}

fn run_froms(fixture: &Fixture, seeds: &[&str], extra: &[&str]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ryii"));
    command.arg("stratify");
    for seed in seeds {
        command.arg("--from").arg(seed);
    }
    command
        .args(extra)
        .arg("--root")
        .arg(&fixture.root)
        .arg(".")
        .current_dir(&fixture.root)
        .output()
        .unwrap()
}

fn rows(stdout: &[u8]) -> Vec<Value> {
    String::from_utf8_lossy(stdout)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn stratify_is_deterministic_and_leaves_the_tree_unchanged() {
    let fixture = fixture("stable");
    let before = tree(&fixture.root);
    let first = run(&fixture, &["--base-lines", "200"]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let second = run(&fixture, &["--base-lines", "200"]);
    assert!(second.status.success());
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(tree(&fixture.root), before);
    let records: Vec<String> = rows(&first.stdout)
        .iter()
        .map(|row| row["record"].as_str().unwrap().to_string())
        .collect();
    assert!(records.contains(&"stratum".to_string()), "{records:?}");
    assert!(
        records.contains(&"stratum_cycle".to_string()),
        "{records:?}"
    );
    assert!(records.contains(&"locality".to_string()), "{records:?}");
    let all_rows = rows(&first.stdout);
    let cycle = all_rows
        .iter()
        .find(|row| row["record"] == "stratum_cycle")
        .unwrap_or_else(|| panic!("{}", String::from_utf8_lossy(&first.stdout)));
    assert_eq!(cycle["members"], 2);
    for row in all_rows.iter().filter(|row| row["record"] == "locality") {
        let internal = row["internal"].as_u64().unwrap();
        let touching = row["touching"].as_u64().unwrap();
        let score = row["score"].as_f64().unwrap();
        let target = row["target_lines"].as_u64().unwrap();
        let expected_score = if touching == 0 {
            0.0
        } else {
            internal as f64 / touching as f64
        };
        assert!((score - expected_score).abs() < 0.0001, "{row}");
        assert_eq!(target, (200.0 * score).round() as u64, "{row}");
    }
    for row in all_rows.iter().filter(|row| row["record"] == "stratum") {
        let prefix = row["prefix"].as_str().unwrap();
        if !prefix.is_empty() {
            assert_eq!(
                row["depth"],
                prefix.trim_end_matches('_').parse::<u32>().unwrap()
            );
        }
    }
    let reasons: Vec<_> = rows(&first.stdout)
        .iter()
        .filter_map(|row| row["reason"].as_str().map(str::to_string))
        .collect();
    for reason in [
        "depth_prefix",
        "merge_candidate",
        "split_candidate",
        "move_candidate",
    ] {
        assert!(
            reasons.contains(&reason.to_string()),
            "missing {reason}; got {reasons:?}; rows {}",
            String::from_utf8_lossy(&first.stdout)
        );
    }
    rows(&first.stdout)
        .into_iter()
        .find(|row| {
            row["record"] == "stratify_move"
                && row["reason"] == "split_candidate"
                && row["cut_lines"]
                    .as_array()
                    .is_some_and(|cut_lines| !cut_lines.is_empty())
        })
        .unwrap_or_else(|| panic!("{}", String::from_utf8_lossy(&first.stdout)));
    let merge = rows(&first.stdout)
        .into_iter()
        .find(|row| row["record"] == "stratify_move" && row["reason"] == "merge_candidate")
        .unwrap();
    assert!(merge["shared_edges"].as_u64().unwrap() > 0);
    let prefix_move = rows(&first.stdout)
        .into_iter()
        .find(|row| row["record"] == "stratify_move" && row["reason"] == "depth_prefix")
        .unwrap();
    let list = fixture.root.parent().unwrap().join("stratify.tsv");
    let state = fixture.root.parent().unwrap().join("state");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(&list, prefix_move["move_tsv"].as_str().unwrap()).unwrap();
    let accepted = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .arg("move")
        .arg("--list")
        .arg(&list)
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&state)
        .current_dir(&fixture.root)
        .output()
        .unwrap();
    assert!(
        accepted.status.success(),
        "ryi move rejected stratify's TSV: {}",
        String::from_utf8_lossy(&accepted.stdout)
    );
    assert_eq!(tree(&fixture.root), before);
}

#[test]
fn stratify_rejects_unknown_edge_kind() {
    let fixture = fixture("bad-kind");
    let output = run(&fixture, &["--kind", "value"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown --kind value"));
}

#[test]
fn entrypoints_union_unranked_paths_and_ranked_paths_choose_the_highest_rank() {
    let fixture = fixture("entrypoints");
    let unranked = run_froms(&fixture, &["main.ts:main", "small.ts:small"], &[]);
    assert!(unranked.status.success());
    let target = rows(&unranked.stdout)
        .into_iter()
        .find(|row| row["record"] == "stratum" && row["path"] == "target.ts")
        .unwrap();
    assert_eq!(target["entry_depth"], 1);
    assert_eq!(target["via"], "small.ts:small");

    let ranked = run_froms(&fixture, &["main.ts:main=1", "small.ts:small=2"], &[]);
    assert!(ranked.status.success());
    let target = rows(&ranked.stdout)
        .into_iter()
        .find(|row| row["record"] == "stratum" && row["path"] == "target.ts")
        .unwrap();
    assert_eq!(target["via"], "small.ts:small");
}
