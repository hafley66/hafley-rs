//! Neutral character-ingest generator.
//!
//! The caller owns identity, display name, ordered source entries and the
//! already-decoded [`HighLevelSubaction`] values. This module pairs them by
//! index, checks that each decoded subaction still carries the declared name,
//! then derives runtime-neutral catalog evidence and the baked [`Action`]s.
//! It never reads HTML, source Rust or the filesystem.
use crate::{Action, bake};
use brawllib_rs::high_level_fighter::HighLevelSubaction;
use serde::{Deserialize, Serialize};

/// One derived catalog row. Every field comes from a decoded action; `file` is
/// copied from the ordered source entry that produced it.
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

/// Runtime-neutral evidence for the whole catalog.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CatalogEvidence {
    pub runtime: String,
    pub display_name: String,
    pub entries: Vec<CatalogEntry>,
}

/// Derived evidence plus the baked actions, both free of parser and
/// filesystem access.
#[derive(Clone, Debug)]
pub struct Catalog {
    pub evidence: CatalogEvidence,
    pub actions: Vec<Action>,
}

/// Why source entries and decoded actions could not be paired.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogError {
    LengthMismatch { sources: usize, actions: usize },
    NameMismatch {
        file: String,
        expected: String,
        actual: String,
    },
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CatalogError::LengthMismatch { sources, actions } => write!(
                f,
                "source entry count {sources} does not match decoded action count {actions}"
            ),
            CatalogError::NameMismatch {
                file,
                expected,
                actual,
            } => write!(f, "{file} contains subaction {actual}, expected {expected}"),
        }
    }
}

impl std::error::Error for CatalogError {}

/// Derive catalog evidence and baked actions from ordered provenance and
/// already-decoded payloads.
///
/// `sources` is `(subaction name, retained file)` in catalog order; `actions`
/// must be decoded in the same order. Index is catalog identity, so an entry is
/// appended without renumbering earlier rows. A decoded action whose name does
/// not match its declared source is rejected rather than silently relabelled.
#[tracing::instrument(target = "game_content::ingest", skip_all, fields(actions = actions.len()))]
pub fn generate_catalog(
    runtime: &str,
    display_name: &str,
    sources: &[(&str, &str)],
    actions: &[HighLevelSubaction],
) -> Result<Catalog, CatalogError> {
    if sources.len() != actions.len() {
        return Err(CatalogError::LengthMismatch {
            sources: sources.len(),
            actions: actions.len(),
        });
    }
    let entries = sources
        .iter()
        .zip(actions)
        .enumerate()
        .map(|(id, ((name, file), action))| {
            if action.name != *name {
                return Err(CatalogError::NameMismatch {
                    file: (*file).into(),
                    expected: (*name).into(),
                    actual: action.name.clone(),
                });
            }
            Ok(CatalogEntry {
                id,
                name: action.name.clone(),
                file: (*file).into(),
                frames: action.frames.len(),
                hurtbox_frames: action.frames.iter().filter(|f| !f.hurt_boxes.is_empty()).count(),
                hitbox_frames: action.frames.iter().filter(|f| !f.hit_boxes.is_empty()).count(),
                iasa: action.iasa,
                landing_lag: action.landing_lag,
            })
        })
        .collect::<Result<Vec<_>, CatalogError>>()?;
    Ok(Catalog {
        evidence: CatalogEvidence {
            runtime: runtime.into(),
            display_name: display_name.into(),
            entries,
        },
        actions: bake(actions),
    })
}
