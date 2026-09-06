// Items as strikeable bodies (plans/item-strikeable.md): the acceptance slice for the second
// `PunchableFace` impl. Punch a ground item -> hp chips + it gets knocked back; hp to 0 -> the
// slot dies; a mounted fixture, your own held tool, and an `item_hp == 0` kind are all immune.
// Drives the `items::hurt::item_strikes` pass directly (a fighter mid-Fsmash at its box's start
// frame, one item sitting under the box), so the one-hit start-frame gate fires exactly once.

use super::*;
use crate::v1::items::hurt::{item_hp, item_strikes};

/// p0 grounded mid-Fsmash at its first box's start frame, with `kind` sitting exactly under that
/// box. Returns the state, the per-fighter tunes `item_strikes` reads, and the item's slot.
fn attacker_over_item(
    kind: ItemKind,
    owner: i8,
    mount: i8,
) -> (SimState, [Tune; MAX_PLAYERS], usize) {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    s.fighters[0].state = CharState::Fsmash;
    s.fighters[0].pos = Vector2::new(600.0, GROUND_Y);
    s.fighters[0].facing = 1.0;
    let hb = t.fsmash.boxes[0];
    s.fighters[0].frame = hb.start; // first active frame: the one-hit-per-swing gate opens here
    let (hc, _hr) = hitbox_center(&s.fighters[0], &hb);
    let slot = 0;
    s.items[slot] = Item {
        kind,
        pos: hc,
        owner,
        mount,
        hp: item_hp(kind),
        ..Item::EMPTY
    };
    let tunes: [Tune; MAX_PLAYERS] = core::array::from_fn(|p| t.for_char(s.fighters[p].char_id));
    (s, tunes, slot)
}

#[test]
fn punch_ground_item_chips_hp_and_knocks_it_back() {
    let (mut s, tunes, slot) = attacker_over_item(ItemKind::LaserGun, -1, -1);
    let hp0 = s.items[slot].hp;
    item_strikes(&mut s, 2, &tunes);
    let it = s.items[slot];
    assert!(it.active(), "one hit shouldn't kill a full-hp gun");
    assert!(it.hp < hp0, "hp chipped: {hp0} -> {}", it.hp);
    assert!(
        it.vel != Vector2::ZERO,
        "knockbackable: the launch wrote velocity"
    );
}

#[test]
fn item_dies_when_hp_hits_zero() {
    let (mut s, tunes, slot) = attacker_over_item(ItemKind::LaserGun, -1, -1);
    s.items[slot].hp = 1.0; // one chip's worth of hp left
    item_strikes(&mut s, 2, &tunes);
    assert!(
        !s.items[slot].active(),
        "hp <= 0 despawns the slot (DeathFx::Despawn)"
    );
}

#[test]
fn mounted_item_is_immune() {
    // `mount >= 0` is checked before the hp gate, so even a strikeable kind (LaserGun) is immune
    // while socketed -- a mounted station is an inert fixture.
    let (mut s, tunes, slot) = attacker_over_item(ItemKind::LaserGun, -1, 3);
    let before = s.items[slot];
    item_strikes(&mut s, 2, &tunes);
    assert!(s.items[slot] == before, "a mounted fixture takes no strike");
}

#[test]
fn own_held_tool_is_immune() {
    // owner == the attacker (0): you can't punch the tool in your own hand.
    let (mut s, tunes, slot) = attacker_over_item(ItemKind::LaserGun, 0, -1);
    let before = s.items[slot];
    item_strikes(&mut s, 2, &tunes);
    assert!(s.items[slot] == before, "your own tool is immune");
}

#[test]
fn zero_hp_kind_is_immune() {
    // A projectile kind (Bomb) has `item_hp == 0` -> opts out of being strikeable entirely.
    let (mut s, tunes, slot) = attacker_over_item(ItemKind::Bomb, -1, -1);
    assert_eq!(item_hp(ItemKind::Bomb), 0.0, "precondition: Bomb opts out");
    let before = s.items[slot];
    item_strikes(&mut s, 2, &tunes);
    assert!(
        s.items[slot] == before,
        "a kind with item_hp 0 takes no strike"
    );
}
