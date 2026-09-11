//! Dog runtime-neutral catalog.
//!
//! The ordered [`CATALOG`] source entries and the single [`SPEC`] are the only
//! declaration of Dog's catalog identity. Decoded PM3.6 payloads flow through
//! the shared `game_content::generate_catalog` for committed evidence and baked
//! actions. Decode and generate require `ingest`; [`load_baked`] is source-free.
#![cfg_attr(not(feature = "ingest"), allow(dead_code))]

/// Runtime namespace.
pub const RUNTIME: &str = "dog";
/// Public display name.
pub const DISPLAY_NAME: &str = "Dog";
/// Number of catalog rows; available without the `ingest` feature.
pub const ACTION_COUNT: usize = 16;

/// Ordered (subaction name, retained file) source entries. Order is the catalog
/// identity. Entries append without renumbering.
#[cfg(feature = "ingest")]
pub const CATALOG: [game_content::SourceEntry<'static>; 16] = [
    game_content::SourceEntry { name: "Wait1", file: "Wait1.html" },
    game_content::SourceEntry { name: "Dash", file: "Dash.html" },
    game_content::SourceEntry { name: "Run", file: "Run.html" },
    game_content::SourceEntry { name: "JumpF", file: "JumpF.html" },
    game_content::SourceEntry { name: "JumpAerialF", file: "JumpAerialF.html" },
    game_content::SourceEntry { name: "Squat", file: "Squat.html" },
    game_content::SourceEntry { name: "SquatWait", file: "SquatWait.html" },
    game_content::SourceEntry { name: "SquatRv", file: "SquatRv.html" },
    game_content::SourceEntry { name: "LandingHeavy", file: "LandingHeavy.html" },
    game_content::SourceEntry { name: "Attack11", file: "Attack11.html" },
    game_content::SourceEntry { name: "Attack12", file: "Attack12.html" },
    game_content::SourceEntry { name: "Attack13", file: "Attack13.html" },
    game_content::SourceEntry { name: "AttackAirF", file: "AttackAirF.html" },
    game_content::SourceEntry { name: "SpecialNStart", file: "SpecialNStart.html" },
    game_content::SourceEntry { name: "SpecialNHold", file: "SpecialNHold.html" },
    game_content::SourceEntry { name: "SpecialNMax", file: "SpecialNMax.html" },
];

/// The one open character spec derived from [`CATALOG`]. No per-character type
/// or enum is introduced.
#[cfg(feature = "ingest")]
pub const SPEC: game_content::CharacterSpec<'static> = game_content::CharacterSpec {
    runtime: RUNTIME,
    display_name: DISPLAY_NAME,
    actions: &CATALOG,
};

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
    use game_content::{Catalog, decode_file};
    use std::path::PathBuf;

    type Error = Box<dyn std::error::Error>;

    pub fn imported_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/fighters/dog/imported")
    }

    /// Decode each catalog payload in ID order, verifying the embedded
    /// subaction name matches the declared catalog name.
    #[tracing::instrument(target = "dog::ingest", skip_all, fields(actions = CATALOG.len()))]
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

    /// Derive committed evidence and baked actions from [`SPEC`] and the
    /// payloads decoded in declared order.
    pub fn generate() -> Result<Catalog, Error> {
        Ok(game_content::generate_catalog(&SPEC, &load()?)?)
    }

    /// Dog fallback policy in this cut: none. Roles Dog retains no exact source
    /// clip for stay explicitly missing; no substitute clip is claimed.
    pub const FALLBACKS: [game_content::RoleFallback<'static>; 0] = [];

    /// Derive role bindings from already-generated catalog evidence. Membership
    /// is mechanical action-name matching; no numeric ID is authored here.
    pub fn generate_roles(
        evidence: &game_content::CatalogEvidence,
    ) -> Result<game_content::RoleBindings, game_content::RoleError> {
        game_content::generate_role_bindings(evidence, &FALLBACKS)
    }
}

#[cfg(feature = "ingest")]
pub use ingest::{generate, generate_roles, imported_dir, load};

#[cfg(test)]
#[path = "2_tests.rs"]
mod tests;
