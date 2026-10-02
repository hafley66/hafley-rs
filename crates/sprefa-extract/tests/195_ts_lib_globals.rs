//! The corpus-unique leg of a TS free call skips every name TypeScript's lib
//! declarations declare; other unbound names keep the documented heuristic.

use std::path::PathBuf;

use sprefa_extract::lang::ts_lib;
use sprefa_extract::{resolve_project, FlatFact, ResolveArms, ResolveRequest, ScipMode, ScipRecords};

#[test]
fn the_lib_declares_dom_and_ecmascript_globals() {
    let globals = ts_lib::globals(None).expect("the bundled TypeScript install (ts7/node_modules)");
    for name in ["URL", "requestAnimationFrame", "fetch", "Map", "Promise", "setTimeout", "Intl"] {
        assert!(globals.contains(name), "{name}");
    }
    assert!(!globals.contains("helper"));
}

#[test]
fn a_lib_global_never_binds_a_corpus_twin() {
    let root = std::env::temp_dir().join(format!("sprefa_ts_lib_globals_{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let caller = root.join("0_globals.ts");
    let twins = root.join("1_twins.ts");
    std::fs::write(
        &caller,
        "export function useGlobals() { requestAnimationFrame(() => {}); return URL(); }\nexport function run() { return helper(); }\n",
    )
    .unwrap();
    std::fs::write(
        &twins,
        "export const requestAnimationFrame = () => 0;\nexport const URL = () => 0;\nexport function helper() { return 1; }\n",
    )
    .unwrap();
    let paths: Vec<PathBuf> = vec![caller, twins];
    let facts = resolve_project(&ResolveRequest {
        paths: &paths,
        arms: ResolveArms { call: true, types: false, flow: false },
        scip: ScipMode::Off,
        project_root: None,
        scip_records: ScipRecords::all(),
        occurrence_text: false,
        rust_checker: None,
        ts_checker: None,
        go_checker: None,
        witness: false,
    })
    .unwrap();
    let edges: Vec<(Option<String>, String)> = facts
        .iter()
        .filter_map(|fact| match fact {
            FlatFact::ResolvedEdge { callee_name, resolution_origin, .. } => {
                Some((callee_name.clone(), resolution_origin.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(edges, vec![(Some("helper".to_string()), "corpus_unique".to_string())]);
    std::fs::remove_dir_all(&root).unwrap();
}
