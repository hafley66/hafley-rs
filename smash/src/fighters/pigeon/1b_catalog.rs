//! Pigeon offline locomotion catalog over fighter-owned Rukaidata PM 3.6 payloads.
//!
//! The ordered [`CATALOG`] source entries and the single [`SPEC`] are the only
//! declaration of Pigeon's catalog identity. Decoded payloads flow through the
//! shared `game_content::generate_catalog` for committed evidence and baked
//! actions. Decode and generate require the `ingest` feature; [`load_baked`] is
//! source-free and works without it.
#[cfg(feature = "ingest")]
use brawllib_rs::high_level_fighter::HighLevelSubaction;
#[cfg(feature = "ingest")]
use game_content::decode_file;
#[cfg(feature = "ingest")]
use std::path::PathBuf;

/// Runtime namespace.
pub const RUNTIME: &str = "pigeon";
/// Public display name.
pub const DISPLAY_NAME: &str = "Private Pigeon";
/// Number of catalog rows; available without the `ingest` feature.
pub const ACTION_COUNT: usize = 22;

#[cfg(feature = "ingest")]
type Error = Box<dyn std::error::Error>;

/// Ordered (subaction name, imported file) source entries. IDs 0-17 are frozen;
/// later entries append without renumbering. IDs 18-21 add basic locomotion
/// clips (crouch enter/hold/exit, backward aerial jump) verified present in the
/// local PM3.6 mirror.
#[cfg(feature = "ingest")]
pub const CATALOG: [game_content::SourceEntry<'static>; 22] = [
    game_content::SourceEntry { name: "Wait1", file: "Wait1.html" },
    game_content::SourceEntry { name: "JumpF", file: "JumpF.html" },
    game_content::SourceEntry { name: "AttackAirF", file: "AttackAirF.html" },
    game_content::SourceEntry { name: "JumpSquat", file: "JumpSquat.html" },
    game_content::SourceEntry { name: "Fall", file: "Fall.html" },
    game_content::SourceEntry { name: "LandingAirF", file: "LandingAirF.html" },
    game_content::SourceEntry { name: "LandingHeavy", file: "LandingHeavy.html" },
    game_content::SourceEntry { name: "WalkSlow", file: "WalkSlow.html" },
    game_content::SourceEntry { name: "WalkMiddle", file: "WalkMiddle.html" },
    game_content::SourceEntry { name: "WalkFast", file: "WalkFast.html" },
    game_content::SourceEntry { name: "Dash", file: "Dash.html" },
    game_content::SourceEntry { name: "Run", file: "Run.html" },
    game_content::SourceEntry { name: "RunBrake", file: "RunBrake.html" },
    game_content::SourceEntry { name: "Turn", file: "Turn.html" },
    game_content::SourceEntry { name: "TurnRun", file: "TurnRun.html" },
    game_content::SourceEntry { name: "JumpB", file: "JumpB.html" },
    game_content::SourceEntry { name: "JumpAerialF", file: "JumpAerialF.html" },
    game_content::SourceEntry { name: "LandingLight", file: "LandingLight.html" },
    game_content::SourceEntry { name: "Squat", file: "Squat.html" },
    game_content::SourceEntry { name: "SquatWait", file: "SquatWait.html" },
    game_content::SourceEntry { name: "SquatRv", file: "SquatRv.html" },
    game_content::SourceEntry { name: "JumpAerialB", file: "JumpAerialB.html" },
];

/// The one open character spec derived from [`CATALOG`]. No per-character type
/// or enum is introduced.
#[cfg(feature = "ingest")]
pub const SPEC: game_content::CharacterSpec<'static> = game_content::CharacterSpec {
    runtime: RUNTIME,
    display_name: DISPLAY_NAME,
    actions: &CATALOG,
};

#[cfg(feature = "ingest")]
fn imported_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/fighters/pigeon/imported")
}

/// Decode each catalog payload in ID order, verifying the embedded subaction name.
#[tracing::instrument(target = "pigeon::ingest", skip_all, fields(actions = CATALOG.len()))]
#[cfg(feature = "ingest")]
pub fn load() -> Result<Vec<HighLevelSubaction>, Error> {
    let root = imported_dir();
    let mut actions = Vec::with_capacity(CATALOG.len());
    for source in CATALOG {
        let action = decode_file(&root.join(source.file))?;
        if action.name != source.name {
            return Err(format!("{} contains subaction {}", source.file, action.name).into());
        }
        actions.push(action);
    }
    Ok(actions)
}

/// Derive committed catalog evidence and baked actions from [`SPEC`] and the
/// payloads decoded in declared order.
#[cfg(feature = "ingest")]
pub fn generate() -> Result<game_content::Catalog, Error> {
    Ok(game_content::generate_catalog(&SPEC, &load()?)?)
}

/// Source-free owned action content embedded at build time. Deserializes
/// `generated/6_baked.json` from the compiled binary; requires no brawllib,
/// HTML, filesystem, or parser access.
pub fn load_baked() -> Result<Vec<game_content::Action>, serde_json::Error> {
    serde_json::from_str(include_str!("generated/6_baked.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical<T: serde::Serialize>(value: &T) -> String {
        format!("{}\n", serde_json::to_string_pretty(value).unwrap())
    }

    #[test]
    fn load_baked_matches_committed_baked() {
        assert_eq!(canonical(&load_baked().unwrap()), include_str!("generated/6_baked.json"));
    }

    #[test]
    fn committed_evidence_covers_contiguous_ids() {
        let evidence: serde_json::Value =
            serde_json::from_str(include_str!("generated/5_catalog.json")).unwrap();
        assert_eq!(evidence["runtime"], RUNTIME);
        assert_eq!(evidence["display_name"], DISPLAY_NAME);
        let entries = evidence["entries"].as_array().unwrap();
        assert_eq!(entries.len(), ACTION_COUNT);
        for (index, entry) in entries.iter().enumerate() {
            assert_eq!(entry["id"].as_u64(), Some(index as u64));
        }
    }

    #[cfg(feature = "ingest")]
    #[test]
    fn catalog_decodes_in_order_with_expected_frames() {
        let actions = load().unwrap();
        assert_eq!(
            actions.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
            CATALOG.map(|source| source.name),
        );
        let frames: Vec<_> = actions.iter().map(|a| a.frames.len()).collect();
        assert_eq!(frames, [
            61, 36, 40, 4, 9, 19, 3, 55, 31, 26, 29, 21, 28, 12, 22, 51, 50, 3,
            8, 61, 10, 40,
        ]);
        assert_eq!(
            (actions[2].iasa, actions[2].landing_lag, actions[5].iasa, actions[6].iasa),
            (Some(35), Some(19.0), Some(19), Some(3)),
        );
    }

    #[cfg(feature = "ingest")]
    #[test]
    fn frozen_ids_and_original_hashes_are_preserved() {
        assert_eq!(
            CATALOG.map(|source| source.name)[..18],
            [
                "Wait1", "JumpF", "AttackAirF", "JumpSquat", "Fall", "LandingAirF",
                "LandingHeavy", "WalkSlow", "WalkMiddle", "WalkFast", "Dash", "Run",
                "RunBrake", "Turn", "TurnRun", "JumpB", "JumpAerialF", "LandingLight",
            ],
        );
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("imported/0_sources.json")).unwrap();
        for (file, hash) in [
            ("Wait1.html", "b5d904b01c4c35630801de7062ed2d495904abb1b4da9d11e97c6dee38f29207"),
            ("JumpF.html", "37a49314e4b7c8b22edd7705238e78f6b5c5cbb77e79d48d4aba882264d51d8a"),
            ("AttackAirF.html", "01b63dfbd97f6cc4c1536f809809164c514adc9fc2b1633f76bc6e22ffe690db"),
            ("JumpSquat.html", "d7937dbf2b6ea50aeba59c0cb2a557f64b4210048ba5cf5d0e3acab32bac6455"),
            ("Fall.html", "b419221f88097a9d7006e1cde2960b0754ee660a0a1e45cd97fe110e8e63e321"),
            ("LandingAirF.html", "5e03b854cc53870ff8380bba8763b6085a8b5d3ddcfdac0cd3f030e09854f3a8"),
            ("LandingHeavy.html", "84187cb92e1c906bfaa0f97f8e5bd7b6e60ff376e5c85f99b581b63773df1dcd"),
            ("WalkSlow.html", "0af16f94c179d65651acbd8bb08e0bb4bb8de118f01dc036a4adee36e2d7f9e9"),
            ("WalkMiddle.html", "a66b6b84498ff78ca2ac56778ef7b4ee65190b50c2d8cd05a94a043e12540605"),
            ("WalkFast.html", "d1eb379dd910fe19d5f6852b2bd1ea5768b50f77677fd4dbd561986fbd51b368"),
            ("Dash.html", "e7f8b6e5a44c894fdac0037c4f76e2cb1afdac1791106a685b595c58ae13aa58"),
            ("Run.html", "0565dfe463cafbbd58344387a973aed2b47aee71677fb138d4347f2dc3ee166d"),
            ("RunBrake.html", "26003a647fe613b8e9aa8fbd198f2bec1c5d9325e5e5ec2d5a46be448bdcdac5"),
            ("Turn.html", "bc6899a4d6e244c8621afdad78c55c03c7785ff6d97605b3beb0fa0bf1560945"),
            ("TurnRun.html", "af93fc0b9fbc047f15a75d569e3bda38ad927452f80a250d833f44917bf3b767"),
            ("JumpB.html", "15d847c7584e7b42f9a414cf572c19e748476f4fb52f1e3119b0a3d0ea4a4da5"),
            ("JumpAerialF.html", "c2b937be09695e4ff2d4761c726c6fe34c56d114dc3bff8ae891803bc80796b1"),
            ("LandingLight.html", "7726bc3c985636a9bf5f6a64b52be9af38b177d5e6f7f63b8f5d1c6204aa7a75"),
        ] {
            assert_eq!(manifest["files"][file], hash, "original hash changed for {file}");
        }
    }

    #[cfg(feature = "ingest")]
    #[test]
    fn appended_locomotion_clips_decode_with_expected_identity() {
        let actions = load().unwrap();
        for (id, name, file, frames) in [
            (18, "Squat", "Squat.html", 8),
            (19, "SquatWait", "SquatWait.html", 61),
            (20, "SquatRv", "SquatRv.html", 10),
            (21, "JumpAerialB", "JumpAerialB.html", 40),
        ] {
            assert_eq!(CATALOG[id].name, name);
            assert_eq!(CATALOG[id].file, file);
            assert_eq!(actions[id].name, name);
            assert_eq!(actions[id].frames.len(), frames);
        }
    }

    #[cfg(feature = "ingest")]
    #[test]
    fn manifest_covers_every_catalog_file() {
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("imported/0_sources.json")).unwrap();
        let files = manifest["files"].as_object().unwrap();
        assert_eq!(files.len(), CATALOG.len() + 1);
        assert!(files.contains_key("attributes.html"));
        for source in CATALOG {
            assert!(files.contains_key(source.file), "missing manifest entry for {}", source.file);
        }
        for source in &CATALOG[18..] {
            assert!(manifest["frames"].get(source.name).is_some(), "missing frame count for {}", source.name);
        }
    }

    /// The committed evidence and baked actions are exactly a fresh decode run
    /// through the shared generator.
    #[cfg(feature = "ingest")]
    #[test]
    fn generated_catalog_evidence_and_bake_are_current() {
        let catalog = generate().unwrap();
        assert_eq!(canonical(&catalog.evidence), include_str!("generated/5_catalog.json"));
        assert_eq!(canonical(&catalog.actions), include_str!("generated/6_baked.json"));
        assert_eq!(catalog.evidence.entries.len(), ACTION_COUNT);
    }

    #[cfg(feature = "ingest")]
    #[test]
    fn embedded_bake_equals_decoded_bake() {
        assert_eq!(
            canonical(&load_baked().unwrap()),
            canonical(&generate().unwrap().actions),
        );
    }
}
