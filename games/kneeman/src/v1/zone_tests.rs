//! The live/toggleable blast zone (queue item): `Tune::zone_mode` gates the fighter KO check
//! (za_warudo.rs) between the static `BLAST_*` frame, that frame unioned with live zone-maker ink
//! (`ZoneMode::InkExtends`), or off entirely. Items are unaffected by any of this -- they always
//! read the static frame, minus a per-kind `ItemConfig.zone_exempt` escape hatch. Declared as a
//! crate-root child so `super::*` is the crate root (same convention as `wings_wear_tests`).

use super::*;

fn tune() -> Tune {
    Tune::from_char(&CharData::KNEEMAN)
}

fn idle() -> InputFrame {
    InputFrame::default()
}

/// `ZoneMode::Off`: a fighter parked well past the static left blast never gets KO'd. Damage and
/// i-frames are the tell -- a KO (`respawn`) zeroes damage and grants `spawn_iframes`; neither
/// happens here.
#[test]
fn zone_off_never_kos_at_the_edges() {
    let mut t = tune();
    t.zone_mode = ZoneMode::Off;
    let mut s = SimState::spawn();
    s.fighters[0].state = CharState::Air;
    s.fighters[0].ground_plat = -1;
    s.fighters[0].pos = Vector2::new(BLAST_LEFT - 200.0, GROUND_Y);
    s.fighters[0].vel = Vector2::ZERO;
    s.fighters[0].damage = 50.0;

    let i = idle();
    let c = step(&s, &[&i, &i], &t);
    assert_eq!(
        c.fighters[0].damage, 50.0,
        "Off: no KO fired, damage untouched"
    );
    assert_eq!(
        c.fighters[0].invuln, 0,
        "Off: no respawn i-frames were granted"
    );
}

/// `ZoneMode::InkExtends`: a still, owned, massive, zone-material stroke (`props.zone`) whose bounding box
/// reaches past the static left blast keeps a fighter out there alive; drop the same stroke and
/// the same fighter position KOs against the static frame `InkExtends` falls back to.
#[test]
fn ink_extends_keeps_a_fighter_alive_past_the_static_frame_and_kos_without_the_ink() {
    let mut t = tune();
    t.zone_mode = ZoneMode::InkExtends;

    let out_x = BLAST_LEFT - 90.0; // past the static frame, but inside the stroke's span below
    let mut with_ink = SimState::spawn();
    with_ink.fighters[0].state = CharState::Air;
    with_ink.fighters[0].ground_plat = -1;
    with_ink.fighters[0].pos = Vector2::new(out_x, GROUND_Y);
    with_ink.fighters[0].vel = Vector2::ZERO;
    with_ink.fighters[0].damage = 50.0;

    // a still zone-material stroke spanning [BLAST_LEFT-150, BLAST_LEFT+50] at GROUND_Y --
    // `ink_blast_zone` only counts STILL (vel == ZERO) player (owner >= 0) bodies (mass > 0.0)
    // stamped with the zone-maker material (`props.zone`); segment classification is irrelevant
    // to it, so this bare two-point path is a legal "zone ink" without ever being finalized.
    let mut zone_ink = InkPath::EMPTY;
    zone_ink.owner = 0;
    zone_ink.mass = 1.0;
    zone_ink.props.zone = true;
    zone_ink.start = with_ink.free.alloc(2).unwrap();
    zone_ink.len = 2;
    zone_ink.pos = Vector2::new(BLAST_LEFT - 150.0, GROUND_Y);
    with_ink.nodes[zone_ink.start as usize].pt = Vector2::ZERO;
    with_ink.nodes[zone_ink.start as usize + 1].pt = Vector2::new(200.0, 0.0);
    with_ink.paths[0] = zone_ink;

    let mut without_ink = with_ink;
    without_ink.paths[0] = InkPath::EMPTY;

    let i = idle();
    let alive = step(&with_ink, &[&i, &i], &t);
    assert_eq!(
        alive.fighters[0].damage, 50.0,
        "the extended zone reaches the fighter: no KO, damage untouched"
    );
    assert_eq!(alive.fighters[0].invuln, 0, "no respawn i-frames granted");

    let ko = step(&without_ink, &[&i, &i], &t);
    assert_eq!(
        ko.fighters[0].damage, 0.0,
        "without the ink, InkExtends falls back to the static frame: this position KOs"
    );
    assert!(
        ko.fighters[0].invuln > 0,
        "a real KO grants respawn i-frames"
    );
}

/// Per-kind item exemption (`ItemConfig.zone_exempt`): an exempt kind's ground item survives
/// resting past the static blast frame where a non-exempt kind at the same spot quiet-despawns.
/// Items ignore `zone_mode` entirely (default `Tune` is `ZoneMode::Static`) -- exemption is the
/// only lever they respect.
#[test]
fn zone_exempt_item_kind_survives_out_of_bounds_where_a_non_exempt_kind_despawns() {
    let mut t = tune();
    t.bomb.zone_exempt = true;

    let mut s = SimState::spawn();
    let out_of_bounds_item = Item {
        kind: ItemKind::BobGun,
        pos: Vector2::new(BLAST_LEFT - 100.0, GROUND_Y),
        gas: 4.0,
        gas_max: 4.0,
        ..Item::EMPTY
    };
    s.items[0] = out_of_bounds_item; // exempt kind (bomb.zone_exempt = true)
    s.items[1] = Item {
        kind: ItemKind::LaserGun, // NOT exempt: laser.zone_exempt stays false
        ..out_of_bounds_item
    };

    let i = idle();
    let c = step(&s, &[&i, &i], &t);
    assert_eq!(
        c.items[0].kind,
        ItemKind::BobGun,
        "zone_exempt bomb kind survives the static-frame despawn"
    );
    assert_eq!(
        c.items[1].kind,
        ItemKind::None,
        "non-exempt laser gun still quiet-despawns off the static frame"
    );
}
