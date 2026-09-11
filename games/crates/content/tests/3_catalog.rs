#![cfg(feature = "ingest")]

use game_content::{
    CatalogError, CharacterSpec, SourceEntry, bake, decode_file, generate_catalog,
};

/// Ordered source entries used for the generator test. `(subaction, retained
/// file)`; the file path is fixture provenance, not API surface.
const SOURCES: [SourceEntry; 3] = [
    SourceEntry {
        name: "Wait1",
        file: "4_pm36_Wait1.html",
    },
    SourceEntry {
        name: "JumpF",
        file: "5_pm36_JumpF.html",
    },
    SourceEntry {
        name: "AttackAirF",
        file: "1_pm36_AttackAirF.html",
    },
];

fn fixture(file: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../blender-godot-sqlite-proof/fixtures/pigeon")
        .join(file)
}

fn decoded() -> Vec<brawllib_rs::high_level_fighter::HighLevelSubaction> {
    SOURCES
        .iter()
        .map(|source| decode_file(&fixture(source.file)).unwrap())
        .collect()
}

fn spec<'a>(runtime: &'a str, display_name: &'a str, actions: &'a [SourceEntry<'a>]) -> CharacterSpec<'a> {
    CharacterSpec {
        runtime,
        display_name,
        actions,
    }
}

#[test]
fn character_catalog_derives_evidence_and_baked_actions_in_source_order() {
    let actions = decoded();
    let catalog = generate_catalog(&spec("probe", "Probe Kit", &SOURCES), &actions).unwrap();

    assert_eq!(catalog.evidence.runtime, "probe");
    assert_eq!(catalog.evidence.display_name, "Probe Kit");
    assert_eq!(catalog.actions.len(), SOURCES.len());
    for (id, (entry, source)) in catalog.evidence.entries.iter().zip(SOURCES).enumerate() {
        assert_eq!(entry.id, id);
        assert_eq!(entry.name, source.name);
        assert_eq!(entry.file, source.file);
        assert_eq!(entry.frames, actions[id].frames.len());
        assert_eq!(
            entry.hurtbox_frames,
            actions[id].frames.iter().filter(|f| !f.hurt_boxes.is_empty()).count(),
        );
        assert_eq!(
            entry.hitbox_frames,
            actions[id].frames.iter().filter(|f| !f.hit_boxes.is_empty()).count(),
        );
        assert_eq!(entry.iasa, actions[id].iasa);
        assert_eq!(entry.landing_lag, actions[id].landing_lag);
    }
    assert_eq!(
        serde_json::to_value(&catalog.actions).unwrap(),
        serde_json::to_value(bake(&actions)).unwrap(),
    );
}

#[test]
fn character_catalog_accepts_synthetic_identity_without_reopening_the_payload() {
    let mut actions = decoded();
    actions[0].name = "SyntheticAction".into();
    let sources = [
        SourceEntry {
            name: "SyntheticAction",
            ..SOURCES[0]
        },
        SOURCES[1],
        SOURCES[2],
    ];
    let catalog = generate_catalog(&spec("probe", "Probe Kit", &sources), &actions).unwrap();
    assert_eq!(catalog.evidence.entries[0].name, "SyntheticAction");
    assert_eq!(catalog.evidence.entries[0].frames, actions[0].frames.len());
}

#[test]
fn character_catalog_rejects_count_and_identity_mismatch() {
    let actions = decoded();
    assert_eq!(
        generate_catalog(&spec("probe", "Probe Kit", &SOURCES), &actions[..2]).unwrap_err(),
        CatalogError::LengthMismatch {
            sources: 3,
            actions: 2
        },
    );

    let mismatched = [SourceEntry {
        name: "NotWait1",
        file: SOURCES[0].file,
    }];
    assert_eq!(
        generate_catalog(&spec("probe", "Probe Kit", &mismatched), &actions[..1]).unwrap_err(),
        CatalogError::NameMismatch {
            file: SOURCES[0].file.into(),
            expected: "NotWait1".into(),
            actual: "Wait1".into(),
        },
    );
}
