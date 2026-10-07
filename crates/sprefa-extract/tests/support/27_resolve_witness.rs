
use serde_json::{json, Value};
use std::path::PathBuf;

const RESOLVE_DIR: &str = "tests/fixtures/resolve";
const TSI_DIR: &str = "tests/fixtures/tsi";

fn of_record<'a>(all: &'a [Value], record: &str) -> Vec<&'a Value> {
    all.iter().filter(|fact| fact["record"] == record).collect()
}

/// `--witness` over `--resolve`: every leg that named a target is a witness on
/// the fact, and a leg that named another def is a fact of its own. The
/// syntax-tier stream is deterministic and freezes whole; the checker tier's
/// normalized tables freeze under the case too (same normalization the old
/// `all__t_98_resolve_witness__stock_*.snap` files pinned).
///
/// SABOTAGE RECEIPT (base sha 8e050ed82): `--witness --resolve` was a clap
/// conflict, `error: the argument '--resolve' cannot be used with '--witness'`,
/// rc=2, and `ProjectEdge` carried no `witnesses` field, so the four checker
/// folds dropped every syntax leg that answered beside the checker.
pub fn evaluate(_case: &Value) -> Value {
    /// One `extract` run from the crate root, stdout as raw lines.
    let lines = |args: &[&str], typescript: Option<String>| {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_ryii"));
        command.current_dir(env!("CARGO_MANIFEST_DIR")).args(args);
        if let Some(path) = typescript {
            command.env("SPREFA_TSGO", path);
        }
        let output = command.output().expect("extract binary runs");
        assert!(
            output.status.success(),
            "{args:?} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    let facts = |args: &[&str], typescript: Option<String>| {
        lines(args, typescript)
            .iter()
            .map(|line| serde_json::from_str::<Value>(line).expect("one json fact per line"))
            .collect::<Vec<_>>()
    };

    // The two ts files the syntax-only cases resolve. `helper` is
    // corpus-unique there, so the name-match leg is the only one that answers.
    let syntax_args: &[&str] = &[
        "--resolve",
        "--arms",
        "call",
        "--root",
        RESOLVE_DIR,
        "tests/fixtures/resolve/0_caller.ts",
        "tests/fixtures/resolve/1_callee.ts",
    ];
    let mut witnessed_syntax_args = vec!["--witness"];
    witnessed_syntax_args.extend_from_slice(syntax_args);

    // A resolve with no --witness is unchanged against the committed golden.
    let golden = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/resolve/2_resolved_edges.jsonl"),
    )
    .expect("the golden is committed");
    let produced = crate::v6_only::ported(&(lines(syntax_args, None).join("\n") + "\n"));
    assert_eq!(produced, golden, "a resolve with no --witness is unchanged");

    // The protocol row opens a witnessed resolve: protocol, then run 0
    // syntax/ryi, and no checker flag means no semantic run.
    let rows = lines(&witnessed_syntax_args, None);
    let head: Vec<Value> = rows[..2]
        .iter()
        .map(|line| serde_json::from_str(line).expect("json"))
        .collect();
    assert_eq!(head[0]["record"], "protocol", "got {}", rows[0]);
    assert_eq!(head[1]["record"], "run", "got {}", rows[1]);
    assert_eq!(head[1]["run"], 0);
    assert_eq!(head[1]["mode"], "syntax");
    assert_eq!(head[1]["tool"], "ryi");

    let syntax = facts(&witnessed_syntax_args, None);
    // Every numbered row carries exactly the legs that named it, and with no
    // checker loaded that is one leg: the row's own `resolution_origin`.
    let edges = of_record(&syntax, "resolved_edge");
    let witnesses = of_record(&syntax, "witness");
    assert!(!edges.is_empty(), "the fixture resolves at least one call");
    assert_eq!(
        edges.len(),
        witnesses.len(),
        "one leg per row on a syntax-only run"
    );
    for edge in &edges {
        let ordinal = edge["fact"].as_u64().expect("every resolved row is numbered");
        let mine: Vec<&&Value> = witnesses
            .iter()
            .filter(|witness| witness["fact"].as_u64() == Some(ordinal))
            .collect();
        assert_eq!(mine.len(), 1, "fact {ordinal} carries one witness");
        assert_eq!(mine[0]["run"], 0);
        assert_eq!(
            mine[0]["method"], edge["resolution_origin"],
            "the method IS the leg the row names"
        );
    }
    // A resolve enumerates no relation, so neither family is complete, and
    // partial coverage carries no diagnostic.
    let mut covered: Vec<(u64, String, String)> = of_record(&syntax, "coverage")
        .iter()
        .map(|row| {
            (
                row["run"].as_u64().unwrap_or_default(),
                row["relation"].as_str().unwrap_or_default().to_string(),
                row["coverage"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    covered.sort();
    assert_eq!(
        covered,
        vec![
            (0, "extract.call".to_string(), "partial".to_string()),
            (0, "extract.type".to_string(), "partial".to_string()),
        ],
    );
    assert!(of_record(&syntax, "diagnostic").is_empty());

    // The witnessed stream survives the reverse door: ingest of the stream
    // equals ingest of the ingest (idempotent canonical form).
    let scratch = std::env::temp_dir().join("sprefa_a2_resolve_witness");
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let raw = scratch.join("stream.jsonl");
    std::fs::write(&raw, lines(&witnessed_syntax_args, None).join("\n") + "\n")
        .expect("write the stream");
    let once = lines(&["ingest", raw.to_str().expect("utf8 path")], None);
    let canonical = scratch.join("once.jsonl");
    std::fs::write(&canonical, once.join("\n") + "\n").expect("write the canonical form");
    let twice = lines(&["ingest", canonical.to_str().expect("utf8 path")], None);
    assert_eq!(once, twice, "the reverse door is idempotent");

    // The CHECKER tier: two tiers, so a fact can carry two witnesses.
    // `agree.ts` imports what it calls and what it extends, so the module
    // plane and the checker land on the same definition at every site.
    let typescript = || crate::stock_tsgo::executable();
    let agree_args: &[&str] = &[
        "--witness",
        "--ts-checker",
        "--resolve",
        "--arms",
        "call,type",
        "--root",
        TSI_DIR,
        "tests/fixtures/tsi/agree.ts",
        "tests/fixtures/tsi/agree_callee.ts",
    ];
    let agree = facts(agree_args, Some(typescript()));
    // One run per tier that ran.
    let mut runs: Vec<(u64, String, String)> = of_record(&agree, "run")
        .iter()
        .map(|row| {
            (
                row["run"].as_u64().unwrap_or_default(),
                row["mode"].as_str().unwrap_or_default().to_string(),
                row["tool"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    runs.sort();
    assert_eq!(
        runs,
        vec![
            (0, "syntax".to_string(), "ryi".to_string()),
            (1, "semantic".to_string(), "tsgo".to_string()),
        ],
        "one run per tier that ran"
    );
    // The resolve legs enumerate no relation, so run 0 covers only the two
    // families partially; the checker WALK is a claim of its own and rides
    // run 1.
    for row in of_record(&agree, "coverage") {
        let relation = row["relation"].as_str().unwrap_or_default();
        let expected = if relation.starts_with("extract.") { 0 } else { 1 };
        assert_eq!(row["run"], expected, "{relation} is covered by the wrong run");
    }
    // `run` calls the imported `helper`. The checker owns the row, the module
    // plane reached the same definition, so ONE fact carries TWO witnesses.
    let edge = of_record(&agree, "resolved_edge")
        .into_iter()
        .find(|row| row["caller_name"] == "run" && row["callee_name"] == "helper")
        .expect("the agreeing call row is in the stream");
    assert_eq!(edge["resolution_origin"], "checker");
    let ordinal = edge["fact"].as_u64().expect("the row is numbered");
    let mut mine: Vec<(u64, String)> = of_record(&agree, "witness")
        .iter()
        .filter(|row| row["fact"].as_u64() == Some(ordinal))
        .map(|row| {
            (
                row["run"].as_u64().unwrap_or_default(),
                row["method"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    mine.sort();
    assert_eq!(
        mine,
        vec![(0, "module_plane".to_string()), (1, "checker".to_string())],
        "the import binding rides the syntax run, the checker its own"
    );

    // `drive` declares a `render` that shadows the import it also carries.
    // The checker names the local one; the syntax plane respects that shadow,
    // and both the lexical and checker legs witness the local shadow.
    let disagree = facts(
        &[
            "--witness",
            "--ts-checker",
            "--resolve",
            "--arms",
            "call",
            "--root",
            TSI_DIR,
            "tests/fixtures/tsi/disagree.ts",
            "tests/fixtures/tsi/disagree_callee.ts",
        ],
        Some(typescript()),
    );
    let shadow_edges = of_record(&disagree, "resolved_edge");
    let mut sited: Vec<(String, String)> = shadow_edges
        .iter()
        .filter(|row| row["caller_site_start"].as_u64() == Some(152))
        .map(|row| {
            (
                row["resolution_origin"].as_str().unwrap_or_default().to_string(),
                row["callee_path"]
                    .as_str()
                    .unwrap_or_default()
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect();
    sited.sort();
    assert_eq!(
        sited,
        vec![("checker".to_string(), "disagree.ts".to_string())],
        "the local shadow names one definition"
    );
    let ordinals: Vec<u64> = shadow_edges
        .iter()
        .filter(|row| row["caller_site_start"].as_u64() == Some(152))
        .filter_map(|row| row["fact"].as_u64())
        .collect();
    assert_eq!(ordinals.len(), 1, "one resolved row has one ordinal");
    for ordinal in &ordinals {
        let mine = of_record(&disagree, "witness")
            .into_iter()
            .filter(|row| row["fact"].as_u64() == Some(*ordinal))
            .count();
        assert_eq!(mine, 2, "the lexical and checker legs witness the local shadow");
    }

    json!({
        "golden_matches": true,
        "protocol_head": [head[0].clone(), head[1].clone()],
        "syntax_stream": syntax,
        "agree_stream": crate::stock_tsgo::normalize(json!(agree)),
        "disagree_stream": crate::stock_tsgo::normalize(json!(disagree)),
    })
}
