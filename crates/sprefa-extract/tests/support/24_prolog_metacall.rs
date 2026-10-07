use serde_json::{json, Value};
use std::process::Command;

/// SWI `meta_predicate` knowledge drives both the call sites and the
/// `reference` positions inside meta-predicate arguments. One `--kinds call`
/// run per fixture corpus and one `--resolve` run over the split def/use pair;
/// every old assert runs as code over the frozen tables' sources BEFORE the
/// snapshot freezes them.
pub fn evaluate(_case: &Value) -> Value {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let kinds = |path: &str| {
        let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
            .arg("--kinds")
            .args(["call", &format!("{manifest}/{path}")])
            .output()
            .expect("extract binary runs");
        assert!(
            output.status.success(),
            "{path}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        parse(&String::from_utf8(output.stdout).expect("stdout is UTF-8"))
    };
    let resolve = |paths: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
            .arg("--resolve")
            .args(paths.iter().map(|path| format!("{manifest}/{path}")))
            .output()
            .expect("extract binary runs");
        assert!(
            output.status.success(),
            "resolve failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("stdout is UTF-8")
    };

    let fixture = kinds("tests/fixtures/prolog/corpus_1_meta_closures.pl");
    let directive = kinds("tests/fixtures/prolog/corpus_3_meta_directive.pl");
    let specs = kinds("tests/fixtures/prolog/corpus_4_meta_specs.pl");
    let resolve_stdout = resolve(&[
        "tests/fixtures/prolog/corpus_2_meta_def.pl",
        "tests/fixtures/prolog/corpus_2_meta_use.pl",
    ]);

    // All four meta slots (maplist/3, call/3, forall/2, findall/3) reach
    // double/2 in clause order; maplist and call slots are closures, forall and
    // findall slots are goals.
    let sites = rows(&fixture, "site", "callee", "double/2");
    assert_eq!(sites.len(), 4, "all four meta slots mint a double/2 site");
    let starts: Vec<u64> = sites.iter().map(|row| row[1].as_u64().unwrap()).collect();
    assert!(
        starts.windows(2).all(|pair| pair[0] < pair[1]),
        "distinct spans in clause order: {starts:?}"
    );
    assert_eq!(
        positions(&fixture, "reference", "functor", "double/2"),
        ["closure", "closure", "goal", "goal"],
        "maplist and call slots are closures, forall and findall slots are goals"
    );
    // forall/2 arg 1 is a goal and recurses.
    assert_eq!(
        positions(&fixture, "reference", "functor", "member/2"),
        ["goal"],
        "forall/2 arg 1 is a goal"
    );
    assert_eq!(
        positions(&fixture, "reference", "functor", "maplist/3"),
        ["goal"]
    );

    // The split def/use pair mints the four go/1 -> double/2 edges through
    // maplist, call, forall, findall.
    let go_to_double = resolve_stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|row| {
            row["record"] == "resolved_edge"
                && row["caller_name"] == "go/1"
                && row["callee_name"] == "double/2"
        })
        .count();
    assert_eq!(
        go_to_double, 4,
        "go/1 -> double/2 through maplist, call, forall, findall: {resolve_stdout}"
    );

    // The file-level meta_predicate directive drives closure slots, and setof
    // unwraps Template^Goal to a goal.
    assert_eq!(
        rows(&directive, "site", "callee", "double2/2").len(),
        1,
        "apply_twice/3's `2` slot mints double2/2 from the bare atom"
    );
    assert_eq!(
        positions(&directive, "reference", "functor", "double2/2"),
        ["closure"],
        "the atom in the file-declared closure slot is a closure reference"
    );
    assert_eq!(
        positions(&directive, "reference", "functor", "parent/2"),
        ["goal"],
        "setof arg 1 unwraps Template^Goal to a goal"
    );

    // setof/bagof templates are data; aggregate_all discriminators are data;
    // goal slots dispatch; partition/include/foldl close over p at arity +2/+3.
    assert_eq!(
        positions(&specs, "reference", "functor", "q/1"),
        ["goal", "goal", "goal", "goal"],
        "setof, aggregate_all, catch_with_backtrace, and setup_call_catcher_cleanup \
         each carry q/1 as a goal reference"
    );
    assert_eq!(
        positions(&specs, "reference", "functor", "p/1"),
        ["term_arg", "term_arg"],
        "setof/bagof templates are data"
    );
    assert!(
        rows(&specs, "site", "callee", "count/0").is_empty(),
        "the aggregate_all discriminator must not become a goal site"
    );
    let r_positions = positions(&specs, "reference", "functor", "r/1");
    assert_eq!(
        r_positions.iter().filter(|position| **position == "goal").count(),
        4,
        "bagof caret, catch_with_backtrace recovery, and both setup_call_catcher_cleanup \
         goal slots: {r_positions:?}"
    );
    assert_eq!(
        positions(&specs, "reference", "functor", "p/2"),
        ["closure", "closure"],
        "partition and include close over p at arity +2"
    );
    assert_eq!(
        positions(&specs, "reference", "functor", "p/3"),
        ["closure"],
        "foldl/4 closes over p at arity +3"
    );

    json!({
        "fixture": {"sites": all(&fixture, "site", "callee"), "references": all(&fixture, "reference", "functor")},
        "directive": {"sites": all(&directive, "site", "callee"), "references": all(&directive, "reference", "functor")},
        "specs": {"sites": all(&specs, "site", "callee"), "references": all(&specs, "reference", "functor")},
        "resolve_edges": resolve_stdout
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|row| row["record"] == "resolved_edge")
            .map(|row| json!([row["caller_name"], row["callee_name"]]))
            .collect::<Vec<_>>(),
    })
}

/// `(name, position, span_start)` for every row of one record kind.
fn all(stdout: &Value, record: &str, key: &str) -> Vec<Value> {
    stdout
        .as_array().unwrap()
        .iter()
        .filter(|row| row["record"] == record)
        .map(|row| {
            json!([
                row[key].as_str().unwrap_or(""),
                row["position"].as_str().unwrap_or(""),
                row["span"]["start"],
            ])
        })
        .collect()
}

fn parse(stdout: &str) -> Value {
    json!(stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Vec<_>>())
}

fn rows(stdout: &Value, record: &str, key: &str, value: &str) -> Vec<Value> {
    stdout
        .as_array().unwrap()
        .iter()
        .filter(|row| row["record"] == record && row[key] == value)
        .map(|row| json!([row["position"], row["span"]["start"]]))
        .collect()
}

fn positions(stdout: &Value, record: &str, key: &str, value: &str) -> Vec<String> {
    rows(stdout, record, key, value)
        .iter()
        .map(|row| row[0].as_str().unwrap_or("").to_string())
        .collect()
}
