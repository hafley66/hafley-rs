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
    let timing: Vec<_> = actions
        .iter()
        .map(|a| a.frames.iter().filter(|f| !f.hit_boxes.is_empty()).count())
        .collect();
    assert_eq!(timing, HITBOX_FRAMES);
    assert_eq!((actions[9].iasa, actions[12].iasa, actions[12].landing_lag), (Some(18), Some(30), Some(18.0)));

    let baked = baked(&actions);
    let baked_timing: Vec<_> = baked
        .iter()
        .map(|a| a.frames.iter().filter(|f| !f.hit_boxes.is_empty()).count())
        .collect();
    assert_eq!(baked_timing, HITBOX_FRAMES);
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
fn manifest_covers_every_retained_payload_with_hash() {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("imported/0_sources.json")).unwrap();
    let files = manifest["files"].as_object().unwrap();
    assert_eq!(files.len(), CATALOG.len());
    for (_, file) in CATALOG {
        let hash = files
            .get(file)
            .unwrap_or_else(|| panic!("missing manifest entry for {file}"))
            .as_str()
            .unwrap();
        assert_eq!(hash.len(), 64, "hash width for {file}");
        assert!(
            hash.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "hash is not lowercase hex for {file}",
        );
    }
    let frames = manifest["frames"].as_object().unwrap();
    assert_eq!(frames.len(), CATALOG.len());
    for ((name, _), count) in CATALOG.iter().zip(FRAMES) {
        assert_eq!(frames[*name].as_u64(), Some(count as u64));
    }
}

#[test]
fn baked_output_is_runtime_neutral() {
    let actions = load().unwrap();
    let content = baked(&actions);
    let json = canonical(&content);
    assert_eq!(json, include_str!("generated/1_baked.json"));
    for forbidden in ["<", "fighter_subaction_data", "base64", "bone_matrix", "hurt_boxes", "script", "bincode"] {
        assert!(!json.contains(forbidden), "baked output contains {forbidden}");
    }
    let restored: Vec<Action> = serde_json::from_str(&json).unwrap();
    assert_eq!(canonical(&restored), json);
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
