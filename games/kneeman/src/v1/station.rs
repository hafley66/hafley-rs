//! Ship stations (plans/lovers-ship.md "v2: stations"): a mounted item socketed to a hull anchor
//! that LOCKS an interacting fighter to that anchor until they jump out. The whole mount seam lives
//! here — anchors, the mounted-item spawn, the reach probe, the occupy actuation, and the frozen
//! per-frame lock the FSM early-returns into — so item.rs / stage / za_warudo carry only thin call
//! sites (kept off .dl/lint-file-budget.dl's ratchet). Pure; re-exported at the crate root.

use crate::v1::body::anchor::{AnchorHost, anchor_pin};
use crate::v1::item::PICKUP_VERT_TOL;
use crate::v1::stage::{SHIP_R, SHIP_SLOT};
use crate::v1::{
    Act, CharState, Fighter, InkPath, InputFrame, Item, ItemKind, MAX_DRAWN, MAX_ITEMS, SimState,
    Tune, Vector2, airborne, hurtbox,
};

/// Station anchors: hull-local offsets in UNIT scale, like `SHIP_RIM` — each scaled by `SHIP_R` (and
/// rotated by the hull's `rot`) at read time in `station_anchor`. Literals, no runtime trig (same
/// reason as `SHIP_RIM`: cos/sin aren't bit-identical across platforms). A station is a mounted item
/// socketed here AND the point a crew member is locked to on occupy. v1 ships ONE: the helm seat,
/// down in the cockpit bowl where a rider already stands.
// parity(v1-ship-station-geometry): SHIP_STATIONS places the helm fixture at its hull-local anchor
pub const SHIP_STATIONS: [Vector2; 1] = [
    Vector2::new(0.0, 0.82), // helm seat: near the bowl floor, dead center under the hatch
];

/// World-space anchor of ship station `idx`: the hull's `pos` plus the hull-local `SHIP_STATIONS`
/// offset scaled by `SHIP_R`, rotated by the hull's `rot`. Mirrors `InkPath::world_pt`'s rotation
/// handling (a plain add while `rot == 0`, i.e. parked), so a spinning/flying hull carries its
/// stations for free with no extra code. None for an out-of-range index or a despawned hull.
pub fn station_anchor(paths: &[InkPath; MAX_DRAWN], idx: usize) -> Option<Vector2> {
    let off = *SHIP_STATIONS.get(idx)? * SHIP_R;
    let hull = &paths[SHIP_SLOT];
    if !hull.active() {
        return None;
    }
    if hull.rot == 0.0 {
        Some(hull.pos + off)
    } else {
        let (s, c) = hull.rot.sin_cos();
        Some(Vector2::new(off.x * c - off.y * s, off.x * s + off.y * c) + hull.pos)
    }
}

/// Nearest mounted STATION item (`mount >= 0`) in interact range of a grounded, actionable fighter.
/// Reuses `nearest_pickup`'s matching rules (the same reach box + grounded/not-stunned gate) but
/// targets the station fixtures instead of pocketable pickups. None in the air / during hitstun /
/// hitlag — occupying is a grounded interact, same as a ground pickup.
pub fn nearest_station(f: &Fighter, items: &[Item; MAX_ITEMS], t: &Tune) -> Option<usize> {
    if airborne(f.state) || f.hitstun != 0 || f.hitlag != 0 {
        return None;
    }
    let (bc, _br) = hurtbox(f);
    items.iter().position(|it| {
        it.active()
            && it.mount >= 0
            && (it.pos.x - bc.x).abs() <= t.pickup_reach
            && (it.pos.y - bc.y).abs() <= PICKUP_VERT_TOL
    })
}

/// Seed a mounted station item socketed to ship anchor `station` (the mount v1 spawn path). Mirrors
/// `bake_ship` seeding the hull: a fixed fixture, not a rolled random spawn. The item sits AT the
/// anchor world pos with `mount = station`, so it never falls/moves/wears/despawns (the `mount >= 0`
/// gate in `update_items`), stays out of the field's one-pickup cap, and a crew member interacting
/// with it OCCUPIES the station. Returns the claimed item slot, or None if the field is full / the
/// hull is missing.
pub fn spawn_station(n: &mut SimState, station: i8) -> Option<usize> {
    let anchor = station_anchor(&n.paths, station as usize)?;
    let slot = n.items.iter().position(|it| !it.active())?;
    n.items[slot] = Item {
        kind: ItemKind::Station,
        pos: anchor,
        mount: station,
        ..Item::EMPTY
    };
    Some(slot)
}

/// Re-pin every mounted station item to its LIVE anchor. Runs AFTER the frame's ink
/// integration + billiard (the hull's pos/rot are final), so the console travels with the
/// flying ship exactly -- not one frame behind (2026-07-04 playtest: the helm item stayed
/// parked where the ship used to be, and occupy reach reads the ITEM's pos). Same
/// `station_anchor` read `stationed_step` pins the seated pilot with.
pub(crate) fn repin_mounted(n: &mut SimState) {
    for k in 0..MAX_ITEMS {
        let m = n.items[k].mount;
        if n.items[k].active() && m >= 0 {
            if let Some(anchor) = station_anchor(&n.paths, m as usize) {
                n.items[k].pos = anchor;
            }
        }
    }
}

/// The occupy INTENT a fighter's FSM emits: empty-handed (the caller already gated that), not
/// already stationed, a mounted station in reach, AND a DELIBERATE grab press -- attack alone
/// never occupies (plans/ship-containment.md #3, 2026-07-06 playtest: "entering the ball has a
/// weird force field"). The station anchor sits right where a crew member naturally lands after
/// dropping through the hatch (`SHIP_STATIONS[0]`'s doc), so an ordinary jab thrown moments after
/// landing used to hijack into piloting -- `occupy` hard-zeroes velocity and freezes the FSM every
/// frame after, reading exactly as an inexplicable snap. Grab is the same "interact" button every
/// other deliberate mount/pickup ritual reads; attack stays free for combat, in or out of the
/// hull. Pure; the one-rider "already taken?" check is cross-fighter and lives in `occupy`.
pub(crate) fn occupy_intent(
    f: &Fighter,
    items: &[Item; MAX_ITEMS],
    t: &Tune,
    grab: bool,
) -> Option<Act> {
    if f.station >= 0 || !grab {
        return None;
    }
    let k = nearest_station(f, items, t)?;
    Some(Act::Occupy {
        station: items[k].mount,
    })
}

/// Actuate `Act::Occupy`: lock fighter `idx` into `station`. One rider per anchor — bail if any OTHER
/// fighter already holds it. Deterministic by handle order (earlier fighters actuated first), so a
/// contested anchor goes to the lower index. No-op on a free/invalid index.
// parity(v1-ship-station-occupancy): occupy and stationed_step mount one fighter, pin it to the live anchor, and release it on jump or launch
pub(crate) fn occupy(n: &mut SimState, idx: usize, station: i8) {
    if station < 0 {
        return;
    }
    let taken = n
        .fighters
        .iter()
        .enumerate()
        .any(|(j, f)| j != idx && f.station == station);
    if taken {
        return;
    }
    n.fighters[idx].station = station;
    n.fighters[idx].vel = Vector2::ZERO;
}

/// One frozen frame of a stationed fighter (called from `reduce_next_state`'s top-of-step
/// early-return, the same spot `Grabbed` freezes). `pos` follows the anchor exactly the way a grabbed
/// victim follows its grabber; the FSM is otherwise skipped. Jump releases (pop out the hatch); a
/// launch (hitstun, from a strike this frame) also knocks the rider loose so the slide owns them next
/// frame. Mutates `n` in place; the caller writes it back and returns `Act::None`.
///
/// Second migrated caller of `body::anchor::anchor_pin` (plans/architecture-debt.md #1; first was
/// `step::repin_ink_riders`). Station-mount has no incremental carry to correct -- unlike the
/// ship-rider case, this fighter isn't already partway-applied a stale surface velocity earlier
/// in the tick, it's simply pinned outright to wherever `station_anchor` reads THIS tick (still
/// one frame behind the hull's own ink integration, same as before this refactor: this runs in
/// the FSM phase, before `integrate_ink`/`resolve_ink_billiard` finalize the hull's pos/rot for
/// the tick -- see `seated_rider_tracks_the_flying_hull`). `anchor_pin`'s general shape (rider +
/// (host's real delta - carry already applied)) collapses to a plain overwrite when the
/// "snapshot" IS the rider's own current position and no carry was pre-applied: `host.pos_now -
/// host.pos_snapshot` is then just `anchor - n.pos`, so `n.pos + (anchor - n.pos) - 0 == anchor`,
/// byte-identical to the old direct assignment. `host.active` mirrors `station_anchor`'s `Option`
/// (a despawned/out-of-range hull leaves the fighter's `pos` untouched, same as the old `if let
/// Some`); `host.traveling` is unconditionally `true` -- there is no kinematic-fixture case to
/// scrub here, every read is a fresh this-tick anchor, not a carried-forward stale one.
pub(crate) fn stationed_step(
    n: &mut Fighter,
    i: &InputFrame,
    paths: &[InkPath; MAX_DRAWN],
    t: &Tune,
) {
    if i.jump || i.shorthop || n.hitstun > 0 {
        n.station = -1;
        n.ground_ink = -1;
        n.ground_plat = -1;
        if i.jump || i.shorthop {
            n.vel.y = t.fullhop_v; // pop up out of the seat, the way you jump out of a grab
            n.state = CharState::Air; // (a launch keeps its Launched state for the slide)
        }
        return;
    }
    let anchor = station_anchor(paths, n.station as usize);
    let host = AnchorHost {
        pos_snapshot: n.pos,
        pos_now: anchor.unwrap_or(n.pos), // unused when `active` is false; see doc above
        active: anchor.is_some(),
        traveling: true,
    };
    n.pos = anchor_pin(n.pos, host, Vector2::ZERO);
    n.vel = Vector2::ZERO;
    n.frame += 1;
}
