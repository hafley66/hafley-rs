//! GENERATED (core-rx-refactor.md row 9): `spec_for`/`dispatch_tick` dispatch, derived
//! from every `impl ItemBehavior for <Variant>Kind` block across `core/src/items/*.rs`
//! by `.dl/gen-items-registry.dl`. Re-exported as `behavior::spec_for` /
//! `behavior::dispatch_tick` so every existing call site is untouched.
//!
//! Regen: `just regen-items-registry` (runs `dl .dl/gen-items-registry.dl --root .`;
//! convergent -- a no-op write when the derived arms already match). Staleness gated by
//! `.dl/registry-staleness.dl` under `dl --check`: a kind file added/removed/renamed
//! without a regen fails the pre-commit rail. Hand-edit ONLY outside the
//! `sprefa:gen`/`sprefa:end` marker pairs below -- everything between them is
//! overwritten on the next regen.

use super::ac_core::AcCoreKind;
use super::behavior::{ItemBehavior, ItemCx, ItemFx, ItemSpec, NoneKind, StationKind};
use super::bomb::{BobGunKind, BombKind};
use super::ink_gun::InkGunKind;
use super::laser_gun::{LaserBoltKind, LaserGunKind};
use super::pen::PenKind;
use super::plasma::PlasmaBallKind;
use super::rocket::RocketKind;
use super::tetris_drop::TetrisDropperKind;
use super::tetris_gun::TetrisGunKind;
use super::wings::WingsBadgeKind;
use crate::v1::{Item, ItemKind};

/// The one dispatch match for the data row. The only other shared merge point besides
/// the `ItemKind` enum itself and `dispatch_tick` below (mod-api.md Tier 0's registry).
pub(crate) fn spec_for(kind: ItemKind) -> ItemSpec {
    match kind {
        // sprefa:gen spec-arms
        ItemKind::AcCore => AcCoreKind.spec(),
        ItemKind::BobGun => BobGunKind.spec(),
        ItemKind::Bomb => BombKind.spec(),
        ItemKind::InkGun => InkGunKind.spec(),
        ItemKind::LaserBolt => LaserBoltKind.spec(),
        ItemKind::LaserGun => LaserGunKind.spec(),
        ItemKind::None => NoneKind.spec(),
        ItemKind::Pen => PenKind.spec(),
        ItemKind::PlasmaBall => PlasmaBallKind.spec(),
        ItemKind::Rocket => RocketKind.spec(),
        ItemKind::Station => StationKind.spec(),
        ItemKind::TetrisDropper => TetrisDropperKind.spec(),
        ItemKind::TetrisGun => TetrisGunKind.spec(),
        ItemKind::WingsBadge => WingsBadgeKind.spec(),
        // sprefa:end
    }
}

/// The one dispatch match for the per-tick hook. `update_items` calls this only for
/// kinds whose tick isn't already generic (held-tool follow/settle, badge settle stay
/// literal in `update_items`, gated by `spec_for`'s classifiers instead). Kinds with no
/// `on_tick` override fall through the trailing wildcard (hand-written, not generated --
/// it is not a per-kind arm).
pub(crate) fn dispatch_tick(kind: ItemKind, it: Item, cx: &mut ItemCx) -> (Item, ItemFx) {
    match kind {
        // sprefa:gen tick-arms
        ItemKind::AcCore => AcCoreKind.on_tick(it, cx),
        ItemKind::Bomb => BombKind.on_tick(it, cx),
        ItemKind::LaserBolt => LaserBoltKind.on_tick(it, cx),
        ItemKind::PlasmaBall => PlasmaBallKind.on_tick(it, cx),
        ItemKind::Rocket => RocketKind.on_tick(it, cx),
        // sprefa:end
        _ => (it, ItemFx::None),
    }
}
