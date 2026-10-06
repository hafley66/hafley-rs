#![cfg(feature = "rust-checker")]
use hafley_scm::read::cache::RETAIN_PROJECT_EXTRACTIONS;
use hafley_scm::read::project::{Planes, read_inputs_with_modules};
use std::path::Path;
use std::sync::{Arc, atomic::Ordering};

#[test]
fn resident_inputs_reuse_only_matching_content() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rust_names_cache");
    let scratch = tempfile::tempdir().unwrap();
    let source = scratch.path().join("0_root.rs");
    let paths = [source.clone()];
    let mut previous = None;
    let mut rows = Vec::new();
    for (state, file, planes) in [
        ("cold", "0_root.rs", Planes::All),
        ("warm", "0_root.rs", Planes::All),
        ("edited", "1_edited.rs", Planes::All),
        ("edited warm", "1_edited.rs", Planes::All),
        ("narrow", "1_edited.rs", Planes::TargetCall),
        ("narrow warm", "1_edited.rs", Planes::TargetCall),
    ] {
        std::fs::copy(fixture.join(file), &source).unwrap();
        RETAIN_PROJECT_EXTRACTIONS.store(true, Ordering::Relaxed);
        let resident = read_inputs_with_modules(&paths, planes)
            .unwrap()
            .pop()
            .unwrap();
        RETAIN_PROJECT_EXTRACTIONS.store(false, Ordering::Relaxed);
        let one_shot = read_inputs_with_modules(&paths, planes)
            .unwrap()
            .pop()
            .unwrap();
        let hit = previous
            .as_ref()
            .is_some_and(|prior| Arc::ptr_eq(prior, &resident.output));
        rows.push((
            state,
            hit,
            resident.blob == one_shot.blob,
            serde_json::to_value(hafley_scm::read::wire::flatten(&resident.output)).unwrap()
                == serde_json::to_value(hafley_scm::read::wire::flatten(&one_shot.output)).unwrap(),
        ));
        previous = Some(resident.output);
    }
    assert_eq!(
        rows,
        vec![
            ("cold", false, true, true),
            ("warm", true, true, true),
            ("edited", false, true, true),
            ("edited warm", true, true, true),
            ("narrow", false, true, true),
            ("narrow warm", true, true, true),
        ]
    );
}
