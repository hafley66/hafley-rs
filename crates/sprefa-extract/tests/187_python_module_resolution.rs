use std::process::Command;

use serde_json::Value;

fn run(fixture: &str, files: &[&str]) -> Vec<Value> {
    let mut args = vec![
        "--resolve".to_string(),
        "--arms".to_string(),
        "call,type".to_string(),
    ];
    args.extend(
        files
            .iter()
            .map(|file| format!("tests/fixtures/{fixture}/{file}")),
    );
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(&args)
        .output()
        .expect("ryii runs");
    assert!(
        output.status.success(),
        "{args:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a flat fact is JSON"))
        .collect()
}

fn type_edges(rows: &[Value]) -> Vec<&Value> {
    rows.iter()
        .filter(|row| row["record"] == "resolved_type_edge")
        .collect()
}

#[test]
fn an_imported_type_resolves_to_its_module_when_the_name_is_ambiguous() {
    let rows = run(
        "python_cross_module_types",
        &["main.py", "models.py", "other.py"],
    );
    assert!(type_edges(&rows).iter().any(|row| {
        row["owner_name"] == "Holder"
            && row["target_name"] == "Widget"
            && row["target_path"] == "tests/fixtures/python_cross_module_types/models.py"
            && row["resolution_origin"] == "module_plane"
    }));
}

#[test]
fn star_import_uses_all_and_reexports_only_listed_names() {
    let rows = run(
        "python_star_all",
        &["main.py", "exports.py", "other.py", "consumer.py"],
    );
    assert!(type_edges(&rows).iter().any(|row| {
        row["owner_name"] == "Holder"
            && row["target_name"] == "Public"
            && row["target_path"] == "tests/fixtures/python_star_all/exports.py"
            && row["resolution_origin"] == "module_plane"
    }));
    assert!(!rows.iter().any(|row| {
        row["record"] == "resolved_import"
            && row["src_path"] == "tests/fixtures/python_star_all/consumer.py"
            && row["local"] == "Hidden"
            && row["target_path"] == "tests/fixtures/python_star_all/exports.py"
    }));
}
