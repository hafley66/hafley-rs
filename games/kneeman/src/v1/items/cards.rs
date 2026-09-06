//! The menu-facing item roster (`ItemCard` + `MENU_ITEMS`). Split from item.rs so the slot/logic
//! file stays under its line ratchet; this is pure presentation data, re-exported at the crate root.

use crate::v1::{ItemKind, StrokeId, ToolKind};

/// A menu-facing description of a spawnable item: which kind, plus the name + one-line blurb the
/// item screen shows, plus the pen loadout (`tool` + `stroke` registry row — two cards can share
/// `ItemKind::Pen` and differ only in material, per the anti-bespoke rule). Host-independent so
/// the shell renders the roster without knowing the kinds.
pub struct ItemCard {
    pub kind: ItemKind,
    pub name: &'static str,
    pub blurb: &'static str,
    pub how: &'static str, // one-line usage tell, shown in the pickup-reach tooltip
    pub tool: ToolKind,
    pub stroke: StrokeId,
}

/// The items the menu offers to spawn. Order here is the order the screen lists them.
pub const MENU_ITEMS: &[ItemCard] = &[
    ItemCard {
        kind: ItemKind::LaserGun,
        name: "Laser Gun",
        blurb: "hold attack to spray flat bolts",
        how: "hold attack: spray bolts",
        tool: ToolKind::TrailPen,
        stroke: 0,
    },
    ItemCard {
        kind: ItemKind::BobGun,
        name: "Bob Gun",
        blurb: "lobs an arcing bomb that blasts",
        how: "attack: lob a bomb",
        tool: ToolKind::TrailPen,
        stroke: 0,
    },
    ItemCard {
        kind: ItemKind::Pen,
        name: "Pen",
        blurb: "tap attack to paint terrain as you move, tap again to set it",
        how: "attack: draw ink, release: done",
        tool: ToolKind::TrailPen,
        stroke: 0,
    },
    ItemCard {
        kind: ItemKind::Pen,
        name: "Zone Pen",
        blurb: "paint permanent zone ink -- still ink of this material extends the live blast zone",
        how: "attack: draw zone ink, release: done",
        tool: ToolKind::TrailPen,
        stroke: crate::v1::StrokeRegistry::ZONE_ROW,
    },
    ItemCard {
        kind: ItemKind::Pen,
        name: "Gate Pen",
        blurb: "paint one-way walls -- bodies pass WITH the ticks, blocked against them",
        how: "attack: draw a one-way wall, release: done",
        tool: ToolKind::TrailPen,
        stroke: crate::v1::StrokeRegistry::GATE_ROW,
    },
    ItemCard {
        kind: ItemKind::InkGun,
        name: "Ink Gun",
        blurb: "tap attack to paint (stick draws), tap again to fire the shape",
        how: "attack: draw a shape, tap again: fire it",
        tool: ToolKind::TrailPen, // unused: anchored drawing has its own cursor
        stroke: 0,
    },
    ItemCard {
        kind: ItemKind::TetrisGun,
        name: "Tetris Gun",
        blurb: "lobs a big blocky boy — stand on it, smack it away",
        how: "attack: lob a block",
        tool: ToolKind::TrailPen, // unused: it's a gun
        stroke: crate::v1::StrokeRegistry::TETRIS_ROW,
    },
    ItemCard {
        kind: ItemKind::WingsBadge,
        name: "Wings",
        blurb: "grab it and it's part of you: air jumps never run out",
        how: "walk up + attack: attach",
        tool: ToolKind::TrailPen, // unused: badges aren't tools
        stroke: 0,
    },
    ItemCard {
        kind: ItemKind::AcCore,
        name: "AC Core",
        blurb: "touch it: body replaced by a mech — boost frame + c-stick arm gun",
        how: "touch it: transform",
        tool: ToolKind::TrailPen, // unused
        stroke: 0,
    },
    ItemCard {
        kind: ItemKind::TetrisDropper,
        name: "Tetris Dropper",
        blurb: "drops a blocky boy flat, straight down right in front of you",
        how: "attack: drop a block (stick up/down at fire picks the piece)",
        tool: ToolKind::TrailPen, // unused: it's a gun
        stroke: crate::v1::StrokeRegistry::TETRIS_ROW,
    },
];
