//! Fast's judge: the scm definitions `ryi fast` emits against a real SCIP
//! index, keyed on (path, name, span). TypeScript is judged; Kotlin is stated
//! unjudged.

#![cfg(feature = "cli")]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const ROOT: &str = "tests/fixtures/ts";

/// One definition either side claims, under the judge key.
type Key = (String, String, u64, u64);

/// The counted split, pinned. `both` is agreement at the exact span; the two
/// one-sided sets are listed by cause below.
const BOTH: usize = 85;
const SCM_ONLY: usize = 0;
const SCIP_ONLY: usize = 50;

/// Why a scip definition has no fast twin. Each bucket is a node kind
/// the vendored helix locals query does not capture, or a symbol that is not
/// a source binding at all.
fn cause(symbol: &str, key: &Key) -> &'static str {
    if key.1.is_empty() && key.2 == 0 && key.3 == 0 {
        return "document symbol: the module itself, no source binding";
    }
    if symbol.ends_with('/') {
        return "namespace or ambient module: no locals capture";
    }
    if symbol.ends_with("().") {
        return "ambient function signature: parsed as function_signature, not function_declaration";
    }
    if object_literal_property(symbol) {
        return "object literal property key: not a lexical binding";
    }
    if symbol.ends_with('#') {
        return "type declaration name (interface, enum, type alias): the scope is captured, its name is not";
    }
    if symbol.contains('#') && symbol.ends_with('.') {
        return "type member (interface property, enum member): not a lexical binding";
    }
    "unclassified"
}

/// scip-typescript spells an object-literal key as `<name><n>:`.
fn object_literal_property(symbol: &str) -> bool {
    let Some(tail) = symbol.rsplit('/').next() else {
        return false;
    };
    let Some(head) = tail.strip_suffix(':') else {
        return false;
    };
    head.ends_with(|ch: char| ch.is_ascii_digit())
        && head.trim_end_matches(|ch: char| ch.is_ascii_digit()).len() < head.len()
}

#[test]
fn the_typescript_definitions_agree_with_scip_typescript() {
    let scm = scm_definitions();
    let scip = scip_definitions();
    let scm_keys: BTreeSet<Key> = scm.keys().cloned().collect();
    let scip_keys: BTreeSet<Key> = scip.keys().cloned().collect();
    let both = &scm_keys & &scip_keys;
    let scm_only: Vec<Key> = (&scm_keys - &scip_keys).into_iter().collect();
    let scip_only: Vec<Key> = (&scip_keys - &scm_keys).into_iter().collect();

    let mut causes: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for key in &scip_only {
        let symbol = &scip[key];
        causes
            .entry(cause(symbol, key))
            .or_default()
            .push(format!("{} {}-{} {:?} {symbol}", key.0, key.2, key.3, key.1));
    }
    let listing = causes
        .iter()
        .map(|(cause, rows)| format!("  {} scip-only: {cause}\n    {}", rows.len(), rows.join("\n    ")))
        .collect::<Vec<_>>()
        .join("\n");
    // @eprintln-ok: the judge prints its split, the way the lab REPORT does.
    eprintln!("ryi fast vs scip-typescript over {ROOT}");
    eprintln!("  both {} scm-only {} scip-only {}", both.len(), scm_only.len(), scip_only.len());
    eprintln!("{listing}");

    assert!(
        scm_only.is_empty(),
        "every fast scm definition is a scip definition at the same span; these are not:\n{scm_only:#?}"
    );
    assert!(
        !causes.contains_key("unclassified"),
        "every scip-only row carries a named cause:\n{listing}"
    );
    assert_eq!(
        (both.len(), scm_only.len(), scip_only.len()),
        (BOTH, SCM_ONLY, SCIP_ONLY),
        "the pinned split moved:\n{listing}"
    );
}

/// No Kotlin indexer exists on this machine, so the Kotlin rows of phase 2 are
/// unjudged. Installing scip-java fails this and asks for the judge.
#[test]
fn the_kotlin_rows_are_unjudged_because_no_kotlin_indexer_is_reachable() {
    let found = Command::new("scip-java").arg("version").output().is_ok();
    assert!(
        !found,
        "scip-java is on PATH now: judge the kotlin rows too, the way this file judges ts"
    );
}

fn scm_definitions() -> BTreeMap<Key, String> {
    let files = ts_files();
    let mut args: Vec<String> = vec!["fast".to_string()];
    args.extend(files.iter().map(|path| path.to_string_lossy().to_string()));
    let rows = ryi(
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        "fast",
    );
    let prefix = format!("{ROOT}/");
    rows.iter()
        .filter(|row| row["record"] == "occurrence" && row["role"] == "def")
        .map(|row| {
            let symbol = row["symbol"].as_str().expect("symbol").to_string();
            let path = row["path"]
                .as_str()
                .expect("path")
                .trim_start_matches(&prefix)
                .to_string();
            let name = symbol
                .rsplit('/')
                .next()
                .and_then(|tail| tail.strip_suffix("()."))
                .expect("the lab symbol spelling")
                .to_string();
            let key = (
                path,
                name,
                row["start"].as_u64().expect("start"),
                row["end"].as_u64().expect("end"),
            );
            (key, symbol)
        })
        .collect()
}

fn ts_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect(Path::new(ROOT), &mut files);
    files.sort();
    files
}

fn collect(path: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).expect("fixture directory") {
        let path = entry.expect("fixture entry").path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("ts") {
            files.push(path);
        }
    }
}

fn scip_definitions() -> BTreeMap<Key, String> {
    let cache = std::env::temp_dir().join(format!("ryi-159-scip-{}", std::process::id()));
    let rows = ryi(
        &[
            "--scip-facts",
            "--scip-build",
            "--occurrence-text",
            "--scip-record",
            "scip_occurrence",
            "--project-root",
            ROOT,
            "--scip-cache",
            &cache.to_string_lossy(),
            "tests/fixtures/ts/sample.ts",
        ],
        "scip",
    );
    rows.iter()
        .filter(|row| row["definition"] == Value::Bool(true))
        .map(|row| {
            let key = (
                row["path"].as_str().expect("path").to_string(),
                row["text"].as_str().unwrap_or_default().to_string(),
                row["start"].as_u64().expect("start"),
                row["end"].as_u64().expect("end"),
            );
            (key, row["symbol"].as_str().expect("symbol").to_string())
        })
        .collect()
}

fn ryi(args: &[&str], slug: &str) -> Vec<Value> {
    let trace: PathBuf = std::env::temp_dir().join(format!(
        "ryi-159-{slug}-{}.json",
        std::process::id()
    ));
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(args)
        .env("HAFLEY_TRACE", trace)
        .env("RUST_LOG", "sprefa_extract=debug")
        .output()
        .expect("ryi runs");
    assert!(
        output.status.success(),
        "ryi {args:?} failed (the judge never fakes green):\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("ryi emits UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("ryi emits JSON"))
        .collect()
}
