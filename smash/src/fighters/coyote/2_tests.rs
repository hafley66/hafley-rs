use super::*;
use game_content::Action;

/// Declared frame counts, one per [`CATALOG`] row. Values come from the raw
/// payload header records and are frozen as deterministic catalog identity.
const FRAMES: [usize; 16] = [
    241, 31, 22, 51, 60, 8, 201, 10, 3, 40, 40, 60, 39, 14, 85, 81,
];

/// Frames carrying at least one hitbox, one per [`CATALOG`] row.
const HITBOX_FRAMES: [usize; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 4, 9, 0, 0, 0];

fn canonical<T: serde::Serialize>(value: &T) -> String {
    format!("{}\n", serde_json::to_string_pretty(value).unwrap())
}

/// SHA256 over exact file bytes, lowercase hex.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

fn imported_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/fighters/coyote/imported")
}

fn committed_evidence() -> CatalogEvidence {
    serde_json::from_str(include_str!("generated/0_catalog.json")).unwrap()
}

fn hitbox_frame_counts(actions: &[Action]) -> Vec<usize> {
    actions
        .iter()
        .map(|action| action.frames.iter().filter(|frame| !frame.hit_boxes.is_empty()).count())
        .collect()
}

#[test]
fn load_baked_matches_embedded_content() {
    let actions = load_baked().unwrap();
    assert_eq!(canonical(&actions), include_str!("generated/1_baked.json"));
}

/// Source-free catalog and frame identity: the embedded content must agree with
/// the declared [`CATALOG`] order, the frozen frame/hitbox counts, and the
/// committed runtime-neutral evidence, with no decode step.
#[test]
fn baked_catalog_and_frame_identity_are_exact() {
    let actions = load_baked().unwrap();
    assert_eq!(actions.len(), CATALOG.len());
    assert_eq!(actions.iter().map(|a| a.frames.len()).collect::<Vec<_>>(), FRAMES);
    assert_eq!(hitbox_frame_counts(&actions), HITBOX_FRAMES);
    assert_eq!(
        (actions[9].iasa, actions[12].iasa, actions[12].landing_lag),
        (Some(18), Some(30), Some(18.0)),
    );

    let evidence = committed_evidence();
    assert_eq!(evidence.runtime, RUNTIME);
    assert_eq!(evidence.display_name, DISPLAY_NAME);
    assert_eq!(evidence.entries.len(), CATALOG.len());
    for (index, (entry, (name, file))) in evidence.entries.iter().zip(CATALOG).enumerate() {
        assert_eq!(entry.id, index);
        assert_eq!(entry.name, name);
        assert_eq!(entry.file, file);
        assert_eq!(entry.frames, FRAMES[index]);
        assert_eq!(entry.hitbox_frames, HITBOX_FRAMES[index]);
        assert_eq!(entry.frames, actions[index].frames.len());
        assert_eq!(entry.hitbox_frames, hitbox_frame_counts(&actions)[index]);
    }
}

#[test]
fn baked_output_is_runtime_neutral() {
    let json = canonical(&load_baked().unwrap());
    assert_eq!(json, include_str!("generated/1_baked.json"));
    for forbidden in ["<", "fighter_subaction_data", "base64", "bone_matrix", "hurt_boxes", "script", "bincode"] {
        assert!(!json.contains(forbidden), "baked output contains {forbidden}");
    }
    let restored: Vec<Action> = serde_json::from_str(&json).unwrap();
    assert_eq!(canonical(&restored), json);
}

/// Recompute SHA256 over every retained payload and compare exact bytes to the
/// manifest; a width or character-class check does not establish identity.
#[test]
fn manifest_covers_every_retained_payload_with_exact_bytes() {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("imported/0_sources.json")).unwrap();
    let files = manifest["files"].as_object().unwrap();
    assert_eq!(files.len(), CATALOG.len());
    let root = imported_root();
    for (name, file) in CATALOG {
        let expected = files
            .get(file)
            .unwrap_or_else(|| panic!("missing manifest entry for {file}"))
            .as_str()
            .unwrap();
        assert_eq!(expected.len(), 64, "hash width for {file}");
        let bytes = std::fs::read(root.join(file)).unwrap();
        assert_eq!(sha256_hex(&bytes), expected, "retained bytes changed for {name} ({file})");
    }
    let frames = manifest["frames"].as_object().unwrap();
    assert_eq!(frames.len(), CATALOG.len());
    for ((name, _), count) in CATALOG.iter().zip(FRAMES) {
        assert_eq!(frames[*name].as_u64(), Some(count as u64));
    }
}

#[test]
fn upstream_identity_is_confined_to_imported_provenance() {
    for neutral in [
        include_str!("1_catalog.rs"),
        include_str!("generated/0_catalog.json"),
        include_str!("generated/1_baked.json"),
    ] {
        assert!(!neutral.contains("Lucario"));
        assert!(!neutral.contains("rukaidata"));
    }
    let manifest = include_str!("imported/0_sources.json");
    assert!(manifest.contains("https://rukaidata.com/PM3.6/Lucario/subactions/"));
}

#[cfg(feature = "ingest")]
mod with_ingest {
    use super::*;

    #[test]
    fn catalog_decodes_in_order_with_deterministic_frame_counts() {
        let actions = load().unwrap();
        assert_eq!(
            actions.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
            CATALOG.map(|(name, _)| name),
        );
        let frames: Vec<_> = actions.iter().map(|a| a.frames.len()).collect();
        assert_eq!(frames, FRAMES);
        // A second decode is byte-for-byte deterministic in frame count.
        assert_eq!(
            load().unwrap().iter().map(|a| a.frames.len()).collect::<Vec<_>>(),
            FRAMES,
        );
    }

    #[test]
    fn hitbox_timing_is_retained_in_decoded_and_baked_content() {
        let actions = load().unwrap();
        let decoded_timing: Vec<_> = actions
            .iter()
            .map(|action| action.frames.iter().filter(|frame| !frame.hit_boxes.is_empty()).count())
            .collect();
        assert_eq!(decoded_timing, HITBOX_FRAMES);
        assert_eq!(
            (actions[9].iasa, actions[12].iasa, actions[12].landing_lag),
            (Some(18), Some(30), Some(18.0)),
        );

        let baked = baked(&actions);
        assert_eq!(hitbox_frame_counts(&baked), HITBOX_FRAMES);
        let attack: Vec<&game_content::Attack> =
            baked[12].frames.iter().flat_map(|f| &f.hit_boxes).collect();
        assert!(!attack.is_empty());
        assert!(attack.iter().all(|a| a.damage.is_finite() && a.radius.is_finite()));
        assert!(attack.iter().any(|a| a.aerial));
    }

    #[test]
    fn decoded_hurtbox_bone_matrices_are_present_and_finite() {
        let actions = load().unwrap();
        for action in &actions {
            for frame in &action.frames {
                assert!(!frame.hurt_boxes.is_empty(), "{} frame without hurtboxes", action.name);
                for hurt in &frame.hurt_boxes {
                    let m = &hurt.bone_matrix;
                    let values = [m.x, m.y, m.z, m.w]
                        .into_iter()
                        .flat_map(|v| [v.x, v.y, v.z, v.w]);
                    assert!(values.into_iter().all(f32::is_finite), "non-finite bone matrix in {}", action.name);
                }
            }
        }
    }

    #[test]
    fn generated_evidence_matches_decoded_catalog() {
        let actions = load().unwrap();
        let record = evidence(&actions);
        assert_eq!(record.runtime, RUNTIME);
        assert_eq!(record.display_name, DISPLAY_NAME);
        assert_eq!(canonical(&record), include_str!("generated/0_catalog.json"));
        assert_eq!(
            record.entries.iter().map(|entry| entry.frames).collect::<Vec<_>>(),
            FRAMES,
        );
    }

    /// The embedded source-free content is exactly the bake of a fresh decode.
    #[test]
    fn embedded_bake_equals_decoded_bake() {
        let decoded = game_content::bake(&load().unwrap());
        assert_eq!(canonical(&load_baked().unwrap()), canonical(&decoded));
    }
}
