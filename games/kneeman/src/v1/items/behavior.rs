//! Sealed, crate-internal, statically dispatched item behavior (plans/mod-api.md Tier
//! 0). This file holds the trait -- its classifier methods DEFAULT, so a kind overrides
//! only the bits it differs on (architecture-debt.md #6) -- the aggregate data row
//! (`ItemSpec`) the trait's default `spec()` assembles from those classifiers, the tick
//! context (`ItemCx`), the effect descriptors (`ItemFx`) that `update_items` (the ONE
//! applier) turns into state, and the two generated dispatch matches (`spec_for`/
//! `dispatch_tick`) that are the only shared merge points besides the `ItemKind` enum
//! itself.
//!
//! Rollback constraints: every impl below is a ZST -- all state stays in the shared
//! `Copy` `Item`; dispatch is a plain match (no dyn, no vtable in state).
//!
//! Trait-with-defaults, not a literal struct each kind builds: before this, a new
//! cross-cutting classifier bit cost one struct-literal field per EXISTING kind (a
//! measured `steered: bool` mechanically touched 13 files -- architecture-debt.md #6,
//! O(existing kinds), forever). As default methods, a new bit is one new method with a
//! default value; only the kinds that need something other than the default override it,
//! so a future bit touches only ITS overriders, never the rest.

use crate::v1::body::Soup;
use crate::v1::combat::{Aim, Swing};
use crate::v1::item::item_landing;
use crate::v1::{
    Badge, DT, FLOOR_LEFT, FLOOR_RIGHT, Fighter, FxKind, Item, ItemConfig, Land, SimState, Tune,
    Vector2, geo, hurtbox, out_of_bounds, strike_ink,
};

use super::act;

// The one dispatch match for the data row and the per-tick hook are GENERATED
// (core-rx-refactor.md row 9): re-exported from `registry_gen` so every existing
// `behavior::spec_for` / `behavior::dispatch_tick` call site is untouched. Regen via
// `just regen-items-registry`; `.dl/registry-staleness.dl` gates drift.
pub(crate) use super::registry_gen::{dispatch_tick, spec_for};

/// How a kind attaches to a fighter, if at all. Classifiers (`is_gun`/`is_pen`/
/// `is_held_tool`/`badge`) derive from this -- one source of truth (mod-api.md Tier 0).
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Attach {
    /// Not attachable: a bare projectile.
    None,
    /// Held in hand on pickup: follows the hand, drops on grab/death (guns, pens).
    Hand,
    /// Consumed on reach + attack via `pickup_item`; grants a fighter badge bit, no
    /// hand slot. AcCore also grants a badge, but by touch (its own `on_tick`), so it
    /// stays `Attach::None` here -- matches `badge()` returning `None` for it today.
    Badge(Badge),
}

/// The data row an item behavior publishes: flight shape (`gravity`/`land`/`aims`/
/// `draws`/`ricochet`/`boom_on_hit`), attach mode, whether it's a transient projectile,
/// and whether an idle empty one unloads on the ground (a spent pen vs. a spent gun,
/// which vanishes instantly on its last shot instead). Classifiers derive from this;
/// `update_items`'s generic held-tool/badge arms read it instead of matching literal
/// kinds. (Panel-tunable numbers -- spawn weight, the hit row -- stay in `Tune`/
/// `ItemConfig`, not duplicated here: this row is only what's truly static per kind.)
///
/// No kind builds this struct directly anymore -- it's the AGGREGATE `ItemBehavior`'s
/// default `spec()` assembles from the classifier methods below. Multi-field readers
/// (`item_logic`, `is_gun`/`is_pen`, item.rs) keep reading it as one value; a new kind
/// only ever touches the individual classifier methods it overrides.
pub(crate) struct ItemSpec {
    pub gravity: bool,
    pub land: Land,
    pub aims: bool,
    pub draws: bool,
    pub ricochet: bool,
    pub boom_on_hit: bool,
    pub is_projectile: bool,
    pub attach: Attach,
    pub despawn_when_spent: bool,
    /// A thrown one in flight can be snatched out of the air by a live grab
    /// (`update_items`' catch pass) instead of hitting. Only meaningful for throwable
    /// hand-tools; projectiles/badges leave it false. Appended at the struct's END
    /// (field-order discipline, same as the sim's serialized structs).
    pub catchable: bool,
}

/// What an `on_tick` hook decided beyond its own returned `Item`; `update_items` is the
/// ONE applier that turns this into state -- collapses the doubled attach path (Wings
/// via `pickup_item`, AcCore via touch) and the Bomb/Rocket explode path into one place.
pub(crate) enum ItemFx {
    /// Nothing beyond the returned `Item`.
    None,
    /// The item is spent; the slot clears.
    Despawn,
    /// Blast at this center (`explode`), then the slot clears.
    Explode(Vector2),
    /// A fighter touched an auto-attach core: grant the badge + roll the arm weapon,
    /// then the slot clears. `fx_at` is the touch point (`FxKind::Transform`).
    Attach {
        fighter: usize,
        badge: Badge,
        arm: u8,
        fx_at: Vector2,
    },
}

/// Everything an `on_tick` hook needs beyond its own `Item`: the rest of `SimState`
/// (fighters to hit, ink to chip, the RNG, the fx ring), the floor/wall soup collected
/// once per `update_items` call, and the active player count. Borrowed fresh per slot.
pub(crate) struct ItemCx<'a> {
    pub n: &'a mut SimState,
    pub t: &'a Tune,
    pub soup: &'a Soup,
    pub np: usize,
}

/// Sealed, crate-internal, statically dispatched (mod-api.md Tier 0). Impls are ZSTs --
/// all state stays in the `Copy` `Item`; a future kind needing more gets a fixed-size
/// `Item` field, never heap. Dispatch is the generated match in `spec_for`/
/// `dispatch_tick` below (no dyn, no vtable in state); a spec-only kind writes none of
/// the hooks.
///
/// `on_touch`/`on_land`/`on_struck` have no caller yet -- today's five kinds with real
/// per-tick logic all fold their contact/landing/strike handling into `on_tick` (folding
/// is possible because the whole state they need is already in `ItemCx`). They stay part
/// of the sealed surface for the next kind that needs a narrower seam than "the whole
/// tick", per mod-api.md's hook list; `#[allow(dead_code)]` until one is exercised.
pub(crate) trait ItemBehavior {
    /// Falls under `Tune.gravity` while airborne. Default true (9 of 14 kinds): override
    /// `false` for a kind that flies dead-flat (the laser bolt, the plasma round) or never
    /// falls at all (`None`/`Station`).
    fn gravity(&self) -> bool {
        true
    }

    /// What a floor/wall contact does to it. Default `Land::Settle` (8 of 14): a ground
    /// pickup. Override `Land::Ignore` for a kind terrain never stops (bolts, the plasma
    /// round, the two spec-only kinds) or `Land::Detonate` for one that blasts on any
    /// floor (Bomb/Rocket).
    fn land(&self) -> Land {
        Land::Settle
    }

    /// C-stick aims this held weapon (arrow tell + directional fire); suppresses
    /// c-attacks. Default false (9 of 14): override true for a held gun.
    fn aims(&self) -> bool {
        false
    }

    /// The attack button draws ink instead of firing (pen-style hold semantics). Default
    /// false (12 of 14): InkGun/Pen override true.
    fn draws(&self) -> bool {
        false
    }

    /// Ink slam reflects it across the incident normal instead of shoving it. Default
    /// false (13 of 14): only the laser bolt overrides true.
    fn ricochet(&self) -> bool {
        false
    }

    /// Ink slam detonates it on the spot. Default false (12 of 14): Bomb/Rocket override
    /// true.
    fn boom_on_hit(&self) -> bool {
        false
    }

    /// A transient hit-effect, not a pickup: doesn't count toward the field's pickup cap
    /// and can never be grabbed. Default false (10 of 14): Bomb/LaserBolt/PlasmaBall/
    /// Rocket override true.
    fn is_projectile(&self) -> bool {
        false
    }

    /// How this kind attaches to a fighter, if at all. Default `Attach::None` (7 of 14):
    /// override `Attach::Hand` for a held tool (the gun/pen family) or `Attach::Badge(_)`
    /// for a touch-consumed passive mod.
    fn attach(&self) -> Attach {
        Attach::None
    }

    /// An idle, empty one unloads off the ground (a spent pen/ink-gun) instead of sitting
    /// forever, unlike a spent gun which vanishes the instant it empties in `fire_gun`.
    /// Default false (12 of 14): InkGun/Pen override true.
    ///
    /// Forward-compat shaping note (plans/move-language.md's fourth noun, `meter`): this
    /// bit IS today's item-side "at-empty" policy for the `Item.gas` use-meter. The
    /// meter's cap/drain rate themselves stay where they are (`Tune`/`fire_gun` --
    /// panel-tunable numbers, out of scope for this trait). A future declarative
    /// `meter { at-empty: ... }` rule would own exactly the boundary action this method
    /// returns; a meter-unification pass can replace this method's body with a lookup
    /// into that declared rule without moving the trait surface or touching any override.
    fn despawn_when_spent(&self) -> bool {
        false
    }

    /// A thrown one in flight can be snatched out of the air by a live grab
    /// (`update_items`'s catch pass) instead of hitting. Default false (10 of 14):
    /// BobGun/InkGun/LaserGun/Pen (the throwable hand tools) override true.
    fn catchable(&self) -> bool {
        false
    }

    /// The aggregate row every existing multi-field caller reads (`spec_for`,
    /// `item_logic`, `is_gun`/`is_pen`/`is_held_tool`/`badge` in item.rs). Default
    /// composes the classifiers above; a kind never builds this struct itself, so adding
    /// a new classifier method never touches an existing kind's file.
    fn spec(&self) -> ItemSpec {
        ItemSpec {
            gravity: self.gravity(),
            land: self.land(),
            aims: self.aims(),
            draws: self.draws(),
            ricochet: self.ricochet(),
            boom_on_hit: self.boom_on_hit(),
            is_projectile: self.is_projectile(),
            attach: self.attach(),
            despawn_when_spent: self.despawn_when_spent(),
            catchable: self.catchable(),
        }
    }

    /// Advance one tick: mutate the item's own fields (a fresh `Item` copy in, one out)
    /// and report the effect `update_items` must apply beyond that.
    fn on_tick(&self, it: Item, cx: &mut ItemCx) -> (Item, ItemFx) {
        let _ = cx;
        (it, ItemFx::None)
    }

    /// A fighter's hurtbox meeting the item outside the tick's own contact sweep.
    #[allow(dead_code)]
    fn on_touch(&self, it: &Item, who: &Fighter) -> ItemFx {
        let _ = (it, who);
        ItemFx::None
    }

    /// The item settled on a floor.
    #[allow(dead_code)]
    fn on_land(&self, it: &mut Item) -> ItemFx {
        let _ = it;
        ItemFx::None
    }

    /// A flying ink body slammed the item.
    #[allow(dead_code)]
    fn on_struck(&self, it: &mut Item, swing: &Swing<'_>) -> ItemFx {
        let _ = (it, swing);
        ItemFx::None
    }
}

/// `ItemKind::None` never reaches a hook (`update_items` skips inactive slots), but the
/// generated dispatch matches (`registry_gen`) stay exhaustive over `ItemKind` rather
/// than pre-filtering it. `pub(crate)`: referenced from `registry_gen`, a sibling module.
pub(crate) struct NoneKind;
impl ItemBehavior for NoneKind {
    fn gravity(&self) -> bool {
        false
    }
    fn land(&self) -> Land {
        Land::Ignore
    }
}

/// A mounted ship station (plans/lovers-ship.md "v2: stations"). Inert as an item: not a
/// projectile, no attach mode (never pocketed — the `mount >= 0` gate in `update_items` skips
/// its whole tick, and a fighter interacting with it OCCUPIES the station instead of pocketing
/// it), no gravity, no landing. Only its `spec()` matters; it writes no per-tick hook.
/// `pub(crate)`: referenced from `registry_gen`, a sibling module.
pub(crate) struct StationKind;
impl ItemBehavior for StationKind {
    fn gravity(&self) -> bool {
        false
    }
    fn land(&self) -> Land {
        Land::Ignore
    }
}

/// Shared shape for a straight-flying burn shot (LaserBolt / PlasmaBall -- mod-api.md's
/// named duplication): fly straight, burn the first non-owner it meets through the one
/// strike ritual, chip ink the same way, fizzle at range or bounds. Kinds differ only in
/// cfg row, aim mode, hitlag weight, damage, and whether a connect pushes an fx.
#[allow(clippy::too_many_arguments)]
pub(crate) fn straight_burn_tick(
    mut it: Item,
    cx: &mut ItemCx,
    cfg: &ItemConfig,
    extra_bounds: bool,
    aim: Aim,
    hitlag_bonus: i64,
    dmg: f32,
    on_hit_fx: Option<FxKind>,
) -> (Item, ItemFx) {
    let p = it.pos + it.vel * DT;
    it.pos = p;
    it.timer -= 1;
    // items always read the static frame here (never `Tune::zone_mode` -- see item.rs's despawn
    // sites): `cfg.zone_exempt` is the only escape hatch.
    let mut spent = it.timer <= 0 || (out_of_bounds(p) && !cfg.zone_exempt);
    if extra_bounds {
        spent = spent || p.x < FLOOR_LEFT - 400.0 || p.x > FLOOR_RIGHT + 400.0;
    }
    for fi in 0..cx.np {
        if fi as i8 == it.owner {
            continue; // your own shots pass through you
        }
        let (bc, br) = hurtbox(&cx.n.fighters[fi]);
        if geo::circles_touch(p, cfg.hit.r, bc, br) {
            // strike write + the on-hit fx (pushed at `p` when the strike connects) relocate to
            // acts::apply_item_act, so this hook never mutates a fighter directly. Applied
            // immediately: bit-identical, and the catch gate still reads the hitstun/hitlag it sets.
            let follow = match on_hit_fx {
                Some(kind) => act::StrikeFollow::Fx { kind, at: p },
                None => act::StrikeFollow::None,
            };
            crate::v1::acts::apply_item_act(
                cx.n,
                act::ItemAct::Strike {
                    fighter: fi as i8,
                    hb: cfg.hit,
                    dmg,
                    kb_scale: 1.0,
                    hitlag_bonus,
                    interrupt: false,
                    aim,
                    from: bc,
                    follow,
                },
                cx.t,
            );
            spent = true;
        }
    }
    if strike_ink(
        &mut cx.n.paths,
        p,
        cfg.hit.r,
        &cfg.hit,
        dmg,
        it.facing,
        &cx.n.nodes,
        cx.t,
    ) {
        spent = true;
    }
    if spent {
        (Item::EMPTY, ItemFx::Despawn)
    } else {
        (it, ItemFx::None)
    }
}

/// No extra terrain check (Bomb's arc has none beyond the shared floor sweep).
pub(crate) fn no_extra_boom(_pre: Vector2, _post: Vector2, _cx: &ItemCx) -> bool {
    false
}

/// Shared shape for an arcing-or-straight blast round (Bomb / Rocket -- mod-api.md's
/// named duplication): advance under `gravity` (0 = straight, like a bazooka round),
/// detonate on fuse-out, any floor the shared landing sweep catches, an `extra_boom`
/// terrain check (Rocket's wall sweep), or grazing any non-owner -- then blast, applied
/// by `update_items` via the returned `ItemFx::Explode`. `Tune.bomb.hit.r` is the contact
/// radius both share (Rocket "detonates like a Bomb").
pub(crate) fn blast_round_tick(
    mut it: Item,
    cx: &mut ItemCx,
    gravity: f32,
    extra_boom: fn(Vector2, Vector2, &ItemCx) -> bool,
) -> (Item, ItemFx) {
    let mut v = it.vel;
    v.y += gravity * DT;
    let p = it.pos + v * DT;
    let landed = item_landing(&it, p, v, cx.soup).is_some();
    let extra = extra_boom(it.pos, p, cx);
    it.pos = p;
    it.vel = v;
    it.timer -= 1;
    let mut boom = it.timer <= 0 || landed || extra;
    for fi in 0..cx.np {
        if fi as i8 == it.owner {
            continue; // doesn't detonate on its own thrower's body in flight
        }
        let (bc, br) = hurtbox(&cx.n.fighters[fi]);
        if geo::circles_touch(p, cx.t.bomb.hit.r, bc, br) {
            boom = true;
        }
    }
    if out_of_bounds(p) && !cx.t.bomb.zone_exempt {
        // fell past a blast zone: quiet despawn, NO explosion. Static frame only (item rule, not
        // `Tune::zone_mode`); `bomb.zone_exempt` covers Rocket too (it shares the Bomb config row).
        (Item::EMPTY, ItemFx::Despawn)
    } else if boom {
        (Item::EMPTY, ItemFx::Explode(p))
    } else {
        (it, ItemFx::None)
    }
}
