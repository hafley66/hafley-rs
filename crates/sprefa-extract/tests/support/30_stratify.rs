use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `ryi stratify`: determinism, record shapes, entrypoint ranking, stem
/// preservation and collision omission, plus the move-TSV round trip through
/// `ryi move`. Every old assert runs as code BEFORE the snapshot freezes the
/// deterministic tables (scores stay live relations; the exact floats are
/// integer math but the contract is the ratio and the 200-line budget).
pub fn evaluate(_case: &Value) -> Value {
    struct Fixture {
        root: PathBuf,
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
    fn fixture(label: &str) -> Fixture {
        let root = std::env::temp_dir().join(format!(
            "sprefa-stratify-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::remove_dir_all(&root).ok();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stratify_ts");
        copy_tree(&source, &root);
        Fixture {
            root: root.canonicalize().unwrap(),
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
    let run = |fixture: &Fixture, extra: &[&str]| {
        run_froms(fixture, &["main.ts:main=1"], extra)
    };
    let rows = |stdout: &[u8]| -> Vec<Value> {
        String::from_utf8_lossy(stdout)
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    };

    // The stable universe: two runs agree byte for byte and leave the tree
    // unchanged; strata, the cycle, localities, the four reasons, and the
    // move-TSV accepted by `ryi move` are all present.
    let fx = fixture("stable");
    let before = tree(&fx.root);
    let first = run(&fx, &["--base-lines", "200"]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let second = run(&fx, &["--base-lines", "200"]);
    assert!(second.status.success());
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(tree(&fx.root), before);
    let all_rows = rows(&first.stdout);
    let record_kinds: Vec<String> = all_rows
        .iter()
        .map(|row| row["record"].as_str().unwrap().to_string())
        .collect();
    for kind in ["stratum", "stratum_cycle", "locality"] {
        assert!(record_kinds.contains(&kind.to_string()), "{record_kinds:?}");
    }
    let cycle = all_rows
        .iter()
        .find(|row| row["record"] == "stratum_cycle")
        .unwrap_or_else(|| panic!("{}", String::from_utf8_lossy(&first.stdout)));
    assert_eq!(cycle["members"], 2);
    let mut strata = Vec::new();
    for row in all_rows.iter().filter(|row| row["record"] == "stratum") {
        let prefix = row["prefix"].as_str().unwrap();
        if !prefix.is_empty() {
            assert_eq!(
                row["depth"],
                prefix.trim_end_matches('_').parse::<u32>().unwrap()
            );
        }
        strata.push(json!([
            row["path"],
            prefix,
            row["depth"],
            row["entry_depth"],
            row["via"],
        ]));
    }
    let mut localities = Vec::new();
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
        localities.push(json!([
            row["path"],
            row["reason"],
            internal,
            touching,
            target,
        ]));
    }
    let reasons: Vec<String> = all_rows
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
    all_rows
        .iter()
        .find(|row| {
            row["record"] == "locality"
                && row["reason"] == "split_candidate"
                && row["cut_lines"].as_array().is_some_and(|cut| !cut.is_empty())
        })
        .unwrap_or_else(|| panic!("{}", String::from_utf8_lossy(&first.stdout)));
    let merge = all_rows
        .iter()
        .find(|row| row["record"] == "locality" && row["reason"] == "merge_candidate")
        .unwrap();
    assert!(merge["shared_edges"].as_u64().unwrap() > 0);
    let prefix_move = all_rows
        .iter()
        .find(|row| row["record"] == "stratify_move" && row["reason"] == "depth_prefix")
        .unwrap();
    let list = fx.root.parent().unwrap().join("stratify.tsv");
    let state = fx.root.parent().unwrap().join("state");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(&list, prefix_move["move_tsv"].as_str().unwrap()).unwrap();
    let accepted = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .arg("move")
        .arg("--list")
        .arg(&list)
        .arg("--root")
        .arg(&fx.root)
        .arg("--state")
        .arg(&state)
        .current_dir(&fx.root)
        .output()
        .unwrap();
    assert!(
        accepted.status.success(),
        "ryi move rejected stratify's TSV: {}",
        String::from_utf8_lossy(&accepted.stdout)
    );
    assert_eq!(tree(&fx.root), before);
    let stable = json!({
        "record_kinds": record_kinds,
        "cycle_members": cycle["members"],
        "strata": strata,
        "localities": localities,
        "reasons": reasons,
        "move_tsv_accepted": true,
    });

    // An unknown --kind is a clap reject with the kind named.
    let fx = fixture("bad-kind");
    let output = run(&fx, &["--kind", "value"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unknown --kind value"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Unranked entrypoints union; ranked paths choose the highest rank.
    let fx = fixture("entrypoints");
    let unranked = run_froms(&fx, &["main.ts:main", "small.ts:small"], &[]);
    assert!(
        unranked.status.success(),
        "{}",
        String::from_utf8_lossy(&unranked.stderr)
    );
    let target = rows(&unranked.stdout)
        .into_iter()
        .find(|row| row["record"] == "stratum" && row["path"] == "target.ts")
        .expect("target.ts stratum");
    assert_eq!(target["entry_depth"], 1);
    assert_eq!(target["via"], "small.ts:small");
    let ranked = run_froms(&fx, &["main.ts:main=1", "small.ts:small=2"], &[]);
    assert!(ranked.status.success());
    let target = rows(&ranked.stdout)
        .into_iter()
        .find(|row| row["record"] == "stratum" && row["path"] == "target.ts")
        .expect("target.ts stratum");
    assert_eq!(target["via"], "small.ts:small");
    let entrypoints = json!({
        "unranked_via": "small.ts:small",
        "unranked_entry_depth": 1,
        "ranked_via": "small.ts:small",
    });

    // Full stems survive the depth prefix; colliding and blocked moves are
    // omitted; every emitted move_tsv is from<TAB>to.
    let fx = fixture("numbered");
    let seeds = [
        "0_log.ts",
        "4_jsxAuto.test.ts",
        "2a_Signal.browser.test.ts",
        "10_history.memory.ts",
        "3_plain.tsx",
        "1_duplicate.ts",
        "2_duplicate.ts",
        "9_blocked.ts",
        "8_directory.ts",
    ];
    for path in seeds.iter().chain(["0_blocked.ts"].iter()) {
        std::fs::write(fx.root.join(path), "export function leaf(){return 1}\n").unwrap();
    }
    // An occupied destination outside the source file set also blocks a move.
    std::fs::create_dir(fx.root.join("0_directory.ts")).unwrap();
    let numbered_first = run_froms(&fx, &seeds, &[]);
    assert!(
        numbered_first.status.success(),
        "{}",
        String::from_utf8_lossy(&numbered_first.stderr)
    );
    let numbered_second = run_froms(&fx, &seeds, &[]);
    assert!(numbered_second.status.success());
    assert_eq!(numbered_first.stdout, numbered_second.stdout);
    let moves: Vec<Value> = rows(&numbered_first.stdout)
        .into_iter()
        .filter(|row| row["record"] == "stratify_move")
        .map(|row| {
            assert_eq!(row["reason"], "depth_prefix");
            assert_eq!(
                row["move_tsv"],
                format!(
                    "{}\t{}",
                    row["from_path"].as_str().unwrap(),
                    row["to_path"].as_str().unwrap()
                )
            );
            json!([row["from_path"], row["to_path"]])
        })
        .collect();
    assert_eq!(
        moves,
        vec![
            json!(["0_log.ts", "0_log.ts"]),
            json!(["10_history.memory.ts", "0_history.memory.ts"]),
            json!(["2a_Signal.browser.test.ts", "0_Signal.browser.test.ts"]),
            json!(["3_plain.tsx", "0_plain.tsx"]),
            json!(["4_jsxAuto.test.ts", "0_jsxAuto.test.ts"]),
        ]
    );

    json!({
        "stable": stable,
        "unknown_kind_rejected": true,
        "entrypoints": entrypoints,
        "numbered_moves": moves,
    })
}
