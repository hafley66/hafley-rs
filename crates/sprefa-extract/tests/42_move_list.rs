//! `extract move --list <tsv>`: three moves touching one importer collapse
//! into one Replace with three edits, a dry run writes nothing, malformed
//! and colliding rows are refused before any stage, and a Replace whose
//! bytes changed between plan and stage is refused. Every original
//! assertion is a step in `tests/fixtures/move_list_cases/`; the soopy
//! stale-stage guard is the one library-level step the CLI cannot reach.
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("move_list_cases", |case| {
        crate::fixture_runner::commands(case, |step| {
            assert_eq!(
                step["api"], "stale_replace_guard",
                "unknown api step: {step}"
            );
            stale_replace_guard(step["root"].as_str().unwrap())
        })
    });
}

/// The guard `bind_action` carries: a Replace states the bytes its offsets
/// were cut against, so an edit landing under it between plan and stage is
/// refused. SABOTAGE RECEIPT: `bind_action` re-reading `expected` off disk
/// measured PASS -> FAIL (the stale stage was accepted).
fn stale_replace_guard(root: &str) -> serde_json::Value {
    let root = std::path::Path::new(root);
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(root.join("one.ts"), "export const one = 1;\n").unwrap();
    let identity = soopy::SourceRoot::open_directory(root)
        .expect("open root")
        .directory()
        .identity
        .clone();
    let source = sprefa_extract::directory_source(&identity, "one.ts");
    let planned = soopy::ContentId::blake3(b"export const one = 1;\n");
    let edit = soopy::TextEdit {
        range: soopy::ActionSpan {
            source: source.clone(),
            start: 19,
            end: 20,
        },
        replacement: b"2".to_vec(),
        producer: soopy::ActionProducer::unordered("extract-move-test"),
    };
    let action = sprefa_extract::replace_action(source, planned, vec![edit]);
    let stage = |root: &std::path::Path,
                 identity: &soopy::DirectoryId,
                 action: &soopy::SourceAction|
     -> Result<(), String> {
        let mut source_root =
            soopy::SourceRoot::open_directory(root).map_err(|error| error.to_string())?;
        let bound = sprefa_extract::bind_action(root, identity, action)?;
        let request = soopy::StageRequest::new(
            soopy::SourceRootId::Directory {
                directory: identity.clone(),
            },
            vec![bound],
        );
        let mut store = soopy::InMemoryStageStore::new();
        soopy::stage_mutations(&mut source_root, &request, &mut store)
            .map(|_| ())
            .map_err(|refusal| refusal.to_string())
    };
    assert!(
        stage(root, &identity, &action).is_ok(),
        "the plan stages while the file still holds the bytes it was cut against"
    );
    std::fs::write(root.join("one.ts"), "export const one = 12345;\n").unwrap();
    assert!(
        stage(root, &identity, &action).is_err(),
        "a Replace bound against stale bytes has to be refused"
    );
    serde_json::json!({"fresh_stage_ok": true, "stale_stage_refused": true})
}
