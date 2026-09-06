//! Per-item-kind behavior (plans/mod-api.md Tier 0: "sealed ItemBehavior trait"). One
//! ZST impl per kind, statically dispatched by the generated matches in
//! `registry_gen::spec_for` / `registry_gen::dispatch_tick` (re-exported as
//! `behavior::spec_for` / `behavior::dispatch_tick` -- every existing caller keeps its
//! path). `item.rs` keeps the registry (the `ItemKind` enum, the spawn table,
//! `MENU_ITEMS`) and `update_items` (the one applier); this module owns each kind's data
//! row (`spec()`) and per-tick logic (`on_tick`).
//!
//! Adding a kind (core-rx-refactor.md row 9): one new file here (a ZST + `impl
//! ItemBehavior`, named `<Variant>Kind`) plus the `ItemKind` variant and a spawn-
//! table/`MENU_ITEMS` row in `item.rs`, then `just regen-items-registry` -- the
//! `spec_for`/`dispatch_tick` arms are generated, never hand-edited.

pub(crate) mod act; // ItemAct/ItemActs: item-tick fighter effects as deferred descriptors
pub(crate) mod behavior;
pub(crate) mod hurt; // items as strikeable bodies: PunchableFace for Item + the strike pass
pub(crate) mod registry_gen; // GENERATED: spec_for/dispatch_tick dispatch (.dl/gen-items-registry.dl)

mod ac_core;
mod bomb;
pub(crate) mod cards; // menu-facing roster data (ItemCard/MENU_ITEMS), re-exported at crate root
pub(crate) mod floor; // free-item floor contact: material row + generic solve (body-unify step 3)
mod ink_gun;
mod laser_gun;
mod pen;
mod plasma;
pub(crate) mod remaps;
mod rocket;
pub(crate) mod tetris_drop; // exposes `shape_from_aim_y` to item.rs's fire_gun, not just spec()
mod tetris_gun;
mod wings;
