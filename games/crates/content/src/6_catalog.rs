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

/// One ordered source entry: the subaction name as it must decode and the
/// retained file it came from. Order is catalog identity. This is provenance
/// input, not derived output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceEntry<'a> {
    pub name: &'a str,
    pub file: &'a str,
}

/// A character's ingest identity and ordered source entries. Open data accepted
/// by [`generate_catalog`]; no per-character type or enum is introduced.
#[derive(Clone, Copy, Debug)]
pub struct CharacterSpec<'a> {
    pub runtime: &'a str,
    pub display_name: &'a str,
    pub actions: &'a [SourceEntry<'a>],
}

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

/// Derive catalog evidence and baked actions from a [`CharacterSpec`] and its
/// already-decoded payloads.
///
/// `spec.actions` is in catalog order; `decoded` must be decoded in the same
/// order. Index is catalog identity, so an entry is appended without
/// renumbering earlier rows. A decoded action whose name does not match its
/// declared source is rejected rather than silently relabelled.
#[tracing::instrument(target = "game_content::ingest", skip_all, fields(actions = decoded.len()))]
pub fn generate_catalog(
    spec: &CharacterSpec<'_>,
    decoded: &[HighLevelSubaction],
) -> Result<Catalog, CatalogError> {
    if spec.actions.len() != decoded.len() {
        return Err(CatalogError::LengthMismatch {
            sources: spec.actions.len(),
            actions: decoded.len(),
        });
    }
    let entries = spec
        .actions
        .iter()
        .zip(decoded)
        .enumerate()
        .map(|(id, (source, action))| {
            if action.name != source.name {
                return Err(CatalogError::NameMismatch {
                    file: source.file.into(),
                    expected: source.name.into(),
                    actual: action.name.clone(),
                });
            }
            Ok(CatalogEntry {
                id,
                name: action.name.clone(),
                file: source.file.into(),
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
            runtime: spec.runtime.into(),
            display_name: spec.display_name.into(),
            entries,
        },
        actions: bake(decoded),
    })
}
