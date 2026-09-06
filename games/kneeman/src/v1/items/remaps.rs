//! Item input remap declarations: which input lanes a held/occupied item intercepts and
//! what UI labels describe them. These are dormant (declared, unconsumed); consumers arrive
//! with the glyph widget and FSM gates per plans/intent-glyph-widgets.md, behind an allowlist
//! expansion in .dl/lint-remaps-confine.dl.
#![allow(dead_code)] // dormant by design: rows land first, consumers land behind the rail change

use crate::v1::ItemKind;

/// An input lane that an item can remap (intercept and repurpose). These are the discrete intent
/// lanes the fighter's FSM gates before any remapping is consulted.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum IntentLane {
    Stick,
    CStick,
    Attack,
    Special,
    Grab,
    Shield,
    Jump,
}

/// A single input remap row: the lane being intercepted and a human-readable label for the UI.
/// The label is static (e.g., "steer", "aim", "boost") and is consumed by the glyph widget
/// to show the player what the remapped input does.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct Remap {
    pub lane: IntentLane,
    pub label: &'static str,
}

/// A set of input remaps for one item kind. Empty means the item does not intercept any
/// input lanes.
#[derive(Copy, Clone)]
pub(crate) struct ItemRemaps(pub &'static [Remap]);

/// Query the remap declaration for an item kind. Must be exhaustive: every `ItemKind`
/// variant must have a deliberate decision here (no `_` arm). New kinds force a conscious
/// choice to add a remap or declare empty.
pub(crate) fn remaps(kind: ItemKind) -> ItemRemaps {
    match kind {
        ItemKind::None => ItemRemaps(&[]),
        ItemKind::LaserGun => ItemRemaps(&[Remap {
            lane: IntentLane::CStick,
            label: "aim",
        }]),
        ItemKind::LaserBolt => ItemRemaps(&[]),
        ItemKind::BobGun => ItemRemaps(&[Remap {
            lane: IntentLane::CStick,
            label: "aim",
        }]),
        ItemKind::Bomb => ItemRemaps(&[]),
        ItemKind::Pen => ItemRemaps(&[]),
        ItemKind::TetrisGun => ItemRemaps(&[Remap {
            lane: IntentLane::CStick,
            label: "aim",
        }]),
        ItemKind::InkGun => ItemRemaps(&[Remap {
            lane: IntentLane::CStick,
            label: "aim",
        }]),
        ItemKind::WingsBadge => ItemRemaps(&[]),
        ItemKind::AcCore => ItemRemaps(&[]),
        ItemKind::Rocket => ItemRemaps(&[]),
        ItemKind::PlasmaBall => ItemRemaps(&[]),
        ItemKind::TetrisDropper => ItemRemaps(&[Remap {
            lane: IntentLane::CStick,
            label: "aim",
        }]),
        ItemKind::Station => ItemRemaps(&[
            Remap {
                lane: IntentLane::CStick,
                label: "steer",
            },
            Remap {
                lane: IntentLane::Attack,
                label: "boost",
            },
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_station_remap() {
        let station_remaps = remaps(ItemKind::Station);
        assert_eq!(station_remaps.0.len(), 2);
        assert_eq!(station_remaps.0[0].lane, IntentLane::CStick);
        assert_eq!(station_remaps.0[0].label, "steer");
        assert_eq!(station_remaps.0[1].lane, IntentLane::Attack);
        assert_eq!(station_remaps.0[1].label, "boost");
    }

    #[test]
    fn test_aiming_kinds() {
        let aiming_kinds = [
            ItemKind::LaserGun,
            ItemKind::BobGun,
            ItemKind::InkGun,
            ItemKind::TetrisGun,
            ItemKind::TetrisDropper,
        ];
        for kind in &aiming_kinds {
            let remap = remaps(*kind);
            assert_eq!(
                remap.0.len(),
                1,
                "aiming kind {:?} should have exactly one remap",
                kind
            );
            assert_eq!(
                remap.0[0].lane,
                IntentLane::CStick,
                "aiming kind {:?} should remap CStick",
                kind
            );
            assert_eq!(
                remap.0[0].label, "aim",
                "aiming kind {:?} should label CStick as 'aim'",
                kind
            );
        }
    }

    #[test]
    fn test_non_aiming_kind() {
        let pen_remaps = remaps(ItemKind::Pen);
        assert_eq!(pen_remaps.0.len(), 0, "Pen should have no remaps");
    }
}
