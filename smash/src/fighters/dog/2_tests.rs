use super::*;
use game_content::Action;

/// Declared frame counts, one per catalog row. Values come from the raw
/// payload header records and are frozen as deterministic catalog identity.
const FRAMES: [usize; 25] = [
    241, 31, 22, 51, 60, 8, 201, 10, 3, 40, 40, 60, 39, 14, 85, 81, 41, 41, 51, 28, 20, 3, 21, 3,
    18,
];

/// Frames carrying at least one hitbox, one per catalog row.
const HITBOX_FRAMES: [usize; 25] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 4, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

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
        .join("src/fighters/dog/imported")
}

/// Source-free mirror of the committed runtime-neutral evidence. Parsed from
/// the generated JSON so the non-ingest tests never reopen a payload.
#[derive(Debug, serde::Deserialize)]
struct Entry {
    id: usize,
    name: String,
    file: String,
    frames: usize,
    hitbox_frames: usize,
}

#[derive(Debug, serde::Deserialize)]
struct Evidence {
    runtime: String,
    display_name: String,
    entries: Vec<Entry>,
}

fn committed_evidence() -> Evidence {
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
/// the frozen frame/hitbox counts and the committed runtime-neutral evidence,
/// with no decode step.
#[test]
fn baked_catalog_and_frame_identity_are_exact() {
    let actions = load_baked().unwrap();
    assert_eq!(actions.len(), ACTION_COUNT);
    assert_eq!(actions.iter().map(|a| a.frames.len()).collect::<Vec<_>>(), FRAMES);
    assert_eq!(hitbox_frame_counts(&actions), HITBOX_FRAMES);
    assert_eq!(
        (actions[9].iasa, actions[12].iasa, actions[12].landing_lag),
        (Some(18), Some(30), Some(18.0)),
    );

    let evidence = committed_evidence();
    assert_eq!(evidence.runtime, RUNTIME);
    assert_eq!(evidence.display_name, DISPLAY_NAME);
    assert_eq!(evidence.entries.len(), ACTION_COUNT);
    let counts = hitbox_frame_counts(&actions);
    for (index, entry) in evidence.entries.iter().enumerate() {
        assert_eq!(entry.id, index);
        assert_eq!(entry.frames, FRAMES[index]);
        assert_eq!(entry.hitbox_frames, HITBOX_FRAMES[index]);
        assert_eq!(entry.frames, actions[index].frames.len());
        assert_eq!(entry.hitbox_frames, counts[index]);
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
/// manifest; a width or character-class check does not establish identity. The
/// committed evidence supplies the ordered names and files, so this test needs
/// no reference to the ingest-gated source rows.
#[test]
fn manifest_covers_every_retained_payload_with_exact_bytes() {
    let evidence = committed_evidence();
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("imported/0_sources.json")).unwrap();
    let files = manifest["files"].as_object().unwrap();
    assert_eq!(files.len(), evidence.entries.len() + 1, "payloads plus attributes.html");
    let root = imported_root();
    for entry in &evidence.entries {
        let expected = files
            .get(&entry.file)
            .unwrap_or_else(|| panic!("missing manifest entry for {}", entry.file))
            .as_str()
            .unwrap();
        assert_eq!(expected.len(), 64, "hash width for {}", entry.file);
        let bytes = std::fs::read(root.join(&entry.file)).unwrap();
        assert_eq!(sha256_hex(&bytes), expected, "retained bytes changed for {} ({})", entry.name, entry.file);
    }
    let frames = manifest["frames"].as_object().unwrap();
    assert_eq!(frames.len(), evidence.entries.len());
    for (entry, count) in evidence.entries.iter().zip(FRAMES) {
        assert_eq!(frames[entry.name.as_str()].as_u64(), Some(count as u64));
    }
}

/// The retained Lucario attributes page is a first-class Dog source: its exact
/// bytes must match the manifest hash, not merely a parse or width check.
#[test]
fn retained_attributes_source_sha_is_exact() {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("imported/0_sources.json")).unwrap();
    let expected = manifest["files"]["attributes.html"].as_str().unwrap();
    assert_eq!(expected.len(), 64, "hash width for attributes.html");
    let bytes = std::fs::read(imported_root().join("attributes.html")).unwrap();
    assert_eq!(sha256_hex(&bytes), expected);
}

/// Parsing the retained page is byte-deterministic and yields only finite
/// numeric rows.
#[test]
fn retained_attributes_parse_deterministically_and_are_finite() {
    let html = include_str!("imported/attributes.html");
    let first = game_content::attributes(html).unwrap();
    let second = game_content::attributes(html).unwrap();
    assert_eq!(first, second);
    assert!(!first.is_empty());
    assert!(first.values().all(|value| value.is_finite()));
}

#[test]
fn upstream_identity_is_confined_to_imported_provenance() {
    for neutral in [
        include_str!("1_catalog.rs"),
        include_str!("generated/0_catalog.json"),
        include_str!("generated/1_baked.json"),
        include_str!("generated/2_attributes.rs"),
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
            CATALOG.map(|source| source.name),
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

        let baked = game_content::bake(&actions);
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

    /// The committed evidence and baked actions are exactly a fresh decode run
    /// through the shared generator.
    #[test]
    fn generated_catalog_evidence_and_bake_are_current() {
        let catalog = generate().unwrap();
        assert_eq!(catalog.evidence.runtime, RUNTIME);
        assert_eq!(catalog.evidence.display_name, DISPLAY_NAME);
        assert_eq!(canonical(&catalog.evidence), include_str!("generated/0_catalog.json"));
        assert_eq!(canonical(&catalog.actions), include_str!("generated/1_baked.json"));
        assert_eq!(
            catalog.evidence.entries.iter().map(|entry| entry.frames).collect::<Vec<_>>(),
            FRAMES,
        );
    }

    /// The embedded source-free content is exactly the bake of a fresh decode.
    #[test]
    fn embedded_bake_equals_decoded_bake() {
        let decoded = generate().unwrap().actions;
        assert_eq!(canonical(&load_baked().unwrap()), canonical(&decoded));
    }
}
