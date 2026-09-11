//! Common Coyote runtime-neutral catalog.
//!
//! Decodes the retained PM3.6 subaction payloads in `imported/` with the shared
//! `game_content` decoder and copies them into owned content with the existing
//! `bake` function. No physics, state transitions, simulation policy, renderer,
//! dependency, or bespoke parser lives here. Upstream character identity is
//! confined to `imported/0_sources.json` and the raw payload bytes.
#![cfg_attr(not(feature = "ingest"), allow(dead_code))]

use serde::{Deserialize, Serialize};

/// Runtime namespace.
pub const RUNTIME: &str = "coyote";
/// Public display name.
pub const DISPLAY_NAME: &str = "Common Coyote";

/// Ordered (subaction name, retained file) pairs. Order is the catalog identity.
/// Entries append without renumbering.
pub const CATALOG: [(&str, &str); 16] = [
    ("Wait1", "Wait1.html"),
    ("Dash", "Dash.html"),
    ("Run", "Run.html"),
    ("JumpF", "JumpF.html"),
    ("JumpAerialF", "JumpAerialF.html"),
    ("Squat", "Squat.html"),
    ("SquatWait", "SquatWait.html"),
    ("SquatRv", "SquatRv.html"),
    ("LandingHeavy", "LandingHeavy.html"),
    ("Attack11", "Attack11.html"),
    ("Attack12", "Attack12.html"),
    ("Attack13", "Attack13.html"),
    ("AttackAirF", "AttackAirF.html"),
    ("SpecialNStart", "SpecialNStart.html"),
    ("SpecialNHold", "SpecialNHold.html"),
    ("SpecialNMax", "SpecialNMax.html"),
];

/// One catalog row as committed to `generated/0_catalog.json`. Values are
/// derived from decoded payloads; only `id`, `name`, and `file` are declared.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: usize,
    pub name: String,
    pub file: String,
    pub frames: usize,
    pub hurtbox_frames: usize,
    pub hitbox_frames: usize,
    pub iasa: Option<usize>,
    pub landing_lag: Option<f32>,
}

/// Runtime-neutral evidence for the whole catalog. Depends only on decoded
/// values; it never names an upstream character or a parser.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CatalogEvidence {
    pub runtime: String,
    pub display_name: String,
    pub entries: Vec<CatalogEntry>,
}

/// Owned, runtime-neutral action content embedded at build time.
///
/// Deserializes `generated/1_baked.json` from the compiled binary. Requires no
/// brawllib, HTML, filesystem, or parser access, so it is usable without the
/// `ingest` feature.
pub fn load_baked() -> Result<Vec<game_content::Action>, serde_json::Error> {
    serde_json::from_str(include_str!("generated/1_baked.json"))
}

#[cfg(feature = "ingest")]
mod ingest {
    use super::*;
    use brawllib_rs::high_level_fighter::HighLevelSubaction;
    use game_content::{Action, decode_file};
    use std::path::PathBuf;

    type Error = Box<dyn std::error::Error>;

    pub fn imported_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/fighters/coyote/imported")
    }

    /// Decode each catalog payload in ID order, verifying the embedded
    /// subaction name matches the declared catalog name.
    #[tracing::instrument(target = "coyote::ingest", skip_all, fields(actions = CATALOG.len()))]
    pub fn load() -> Result<Vec<HighLevelSubaction>, Error> {
        let root = imported_dir();
        let mut actions = Vec::with_capacity(CATALOG.len());
        for (expected, file) in CATALOG {
            let action = decode_file(&root.join(file))?;
            if action.name != expected {
                return Err(format!("{file} contains subaction {}", action.name).into());
            }
            actions.push(action);
        }
        Ok(actions)
    }

    /// Copy decoded actions into owned, runtime-neutral content.
    pub fn baked(actions: &[HighLevelSubaction]) -> Vec<Action> {
        game_content::bake(actions)
    }

    /// Build the committed evidence from decoded actions.
    pub fn evidence(actions: &[HighLevelSubaction]) -> CatalogEvidence {
        CatalogEvidence {
            runtime: RUNTIME.into(),
            display_name: DISPLAY_NAME.into(),
            entries: actions
                .iter()
                .enumerate()
                .map(|(id, action)| CatalogEntry {
                    id,
                    name: action.name.clone(),
                    file: CATALOG[id].1.into(),
                    frames: action.frames.len(),
                    hurtbox_frames: action.frames.iter().filter(|f| !f.hurt_boxes.is_empty()).count(),
                    hitbox_frames: action.frames.iter().filter(|f| !f.hit_boxes.is_empty()).count(),
                    iasa: action.iasa,
                    landing_lag: action.landing_lag,
                })
                .collect(),
        }
    }
}

#[cfg(feature = "ingest")]
pub use ingest::{baked, evidence, imported_dir, load};

#[cfg(test)]
#[path = "2_tests.rs"]
mod tests;
