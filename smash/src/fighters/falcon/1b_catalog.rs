//! Falcon offline locomotion catalog over fighter-owned Rukaidata PM 3.6 payloads.
//! Indices 0..=6 match the runtime's existing seven-action order; later entries
//! extend the catalog without renumbering. Requires the `ingest` feature.
#[cfg(feature = "ingest")]
use game_content::decode_file;
#[cfg(feature = "ingest")]
use brawllib_rs::high_level_fighter::HighLevelSubaction;
#[cfg(feature = "ingest")]
use std::path::PathBuf;

#[cfg(feature = "ingest")]
type Error = Box<dyn std::error::Error>;

/// Ordered (subaction name, imported file) pairs. IDs 0-6 are frozen.
pub const CATALOG: [(&str, &str); 18] = [
    ("Wait1", "Wait1.html"),
    ("JumpF", "JumpF.html"),
    ("AttackAirF", "AttackAirF.html"),
    ("JumpSquat", "JumpSquat.html"),
    ("Fall", "Fall.html"),
    ("LandingAirF", "LandingAirF.html"),
    ("LandingHeavy", "LandingHeavy.html"),
    ("WalkSlow", "WalkSlow.html"),
    ("WalkMiddle", "WalkMiddle.html"),
    ("WalkFast", "WalkFast.html"),
    ("Dash", "Dash.html"),
    ("Run", "Run.html"),
    ("RunBrake", "RunBrake.html"),
    ("Turn", "Turn.html"),
    ("TurnRun", "TurnRun.html"),
    ("JumpB", "JumpB.html"),
    ("JumpAerialF", "JumpAerialF.html"),
    ("LandingLight", "LandingLight.html"),
];

#[cfg(feature = "ingest")]
fn imported_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/fighters/falcon/imported")
}

/// Decode each catalog payload in ID order, verifying the embedded subaction name.
#[tracing::instrument(target = "falcon::ingest", skip_all, fields(actions = CATALOG.len()))]
#[cfg(feature = "ingest")]
pub fn load() -> Result<Vec<HighLevelSubaction>, Error> {
    let root = imported_dir();
    let mut actions = Vec::with_capacity(CATALOG.len());
    for (expected, file) in CATALOG {
        let action = decode_file(&root.join(file))?;
        if action.name != expected {
            return Err(format!("{} contains subaction {}", file, action.name).into());
        }
        actions.push(action);
    }
    Ok(actions)
}

#[cfg(all(test, feature = "ingest"))]
mod tests {
    use super::*;

    #[test]
    fn catalog_decodes_in_order_with_expected_frames() {
        let actions = load().unwrap();
        assert_eq!(actions.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), CATALOG.map(|(name, _)| name));
        let frames: Vec<_> = actions.iter().map(|a| a.frames.len()).collect();
        assert_eq!(frames, [
            61, 36, 40, 4, 9, 19, 3, 55, 31, 26, 29, 21, 28, 12, 22, 51, 50, 3,
        ]);
        assert_eq!(
            (actions[2].iasa, actions[2].landing_lag, actions[5].iasa, actions[6].iasa),
            (Some(35), Some(19.0), Some(19), Some(3)),
        );
    }

    #[test]
    fn manifest_covers_every_catalog_file() {
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("imported/0_sources.json")).unwrap();
        let files = manifest["files"].as_object().unwrap();
        assert_eq!(files.len(), CATALOG.len() + 1);
        assert!(files.contains_key("attributes.html"));
        for (_, file) in CATALOG {
            assert!(files.contains_key(file), "missing manifest entry for {file}");
        }
    }
}
