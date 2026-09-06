// C1 (plans/turnkey-extension.md): characters became data. `CharSpec` is the per-character mod
// unit, `MatchTune` the match-global split; a fighter resolves physics + moveset through its
// `char_id` roster row (`Tune::for_char`). These tests pin (1) KNEEMAN-through-CharSpec reproduces
// the old constants, and (2) a distinct roster row actually drives a distinct fighter. Declared as
// a crate-root child so `super::*` is the crate root.

use super::*;

// ── (1) KNEEMAN through the CharSpec/MatchTune split == the old flat `Tune::from_char` constants.

#[test]
fn default_resolves_the_kneeman_constants() {
    let t = Tune::default();
    // physics: source units -> pixels, exactly as the old from_char did.
    assert_eq!(t.gravity, acc(0.17), "gravity = acc(CharData.gravity)");
    assert_eq!(t.run_speed, vel(2.34), "run_speed = vel(run_max)");
    assert_eq!(t.fullhop_v, -vel(3.68), "jump velocities are negative (up)");
    // character kit resolved from CharSpec source units.
    assert_eq!(t.walljump_v, -vel(2.6), "walljump kit resolves from source");
    assert_eq!(t.crawl_speed, vel(0.45), "crawl kit resolves from source");
    assert_eq!(t.weight, 104.0, "weight is a character stat");
    // match knobs resolved from MatchTune.
    assert_eq!(t.techroll_speed, vel(2.4), "techroll_speed is a match knob");
    assert_eq!(t.booster_kb, vel(2.6), "booster is a match knob");
    assert!(t.items_on, "items default on");
}

#[test]
fn from_char_matches_resolve_of_kneeman_spec() {
    // The kept `Tune::from_char(CharData)` path and the new resolve path agree field for field on
    // the sim-visible knobs (the whole struct minus the non-PartialEq item/stroke configs).
    let a = Tune::from_char(&CharData::KNEEMAN);
    let b = Tune::resolve(&CharSpec::KNEEMAN, &MatchTune::default());
    assert_eq!(a.gravity, b.gravity);
    assert!(a.jab == b.jab, "moveset comes through identically");
    assert!(a.specials == b.specials);
    assert_eq!(a.weight, b.weight);
    assert_eq!(a.di_max_angle, b.di_max_angle);
}

#[test]
fn for_char_row0_is_the_flat_view() {
    // Row 0 is the panel-editable flat view: `for_char(0)` returns it verbatim, so a panel edit to
    // any field (char or match) drives a char-0 fighter with zero re-resolve.
    let mut t = Tune::default();
    t.gravity = 12345.0; // a "panel edit" to a char field
    t.knockback_mult = 9.0; // and a match field
    let v = t.for_char(0);
    assert_eq!(v.gravity, 12345.0);
    assert_eq!(v.knockback_mult, 9.0);
}

#[test]
fn panel_match_edits_carry_into_other_rows() {
    // A match-global panel edit reaches every fighter, including ones on a non-zero roster row
    // (they re-resolve against the live match knobs).
    let mut t = Tune::default();
    std::sync::Arc::make_mut(&mut t.roster)[1].phys.gravity = 0.4; // make row 1 distinct so for_char(1) re-resolves
    t.knockback_mult = 9.0; // panel edits a match knob on the flat view
    assert_eq!(
        t.for_char(1).knockback_mult,
        9.0,
        "row 1 re-resolve keeps the live match edit"
    );
}

// ── (2) the seam is real: a distinct CharSpec row resolves distinct physics + weight per fighter.

#[test]
fn roster_row_drives_distinct_resolution() {
    let mut t = Tune::default();
    let mut heavy = CharSpec::KNEEMAN;
    heavy.phys.gravity = 0.40; // a floatier/heavier feel than KNEEMAN's 0.17
    heavy.weight = 200.0;
    std::sync::Arc::make_mut(&mut t.roster)[1] = heavy;

    assert_eq!(t.for_char(0).gravity, acc(0.17), "row 0 unchanged");
    assert_eq!(
        t.for_char(1).gravity,
        acc(0.40),
        "row 1 uses its own physics"
    );
    assert_ne!(t.for_char(0).gravity, t.for_char(1).gravity);
    assert_eq!(t.weight_of(0), 104.0);
    assert_eq!(t.weight_of(1), 200.0, "weight resolves per character");
}

#[test]
fn char_id_indexing_clamps_out_of_range() {
    // A menu pick past the shell art slots (chars::ART_SLOT_ROW) maps to row 0 (baseline) instead
    // of panicking -- and NOT to the last distinct kit (Lucas), which is the alignment bug this
    // guards against: an out-of-range slot must resolve plain physics, never a floaty outlier.
    let t = Tune::default();
    let big = t.for_char(200);
    assert_eq!(
        big.gravity,
        t.for_char(0).gravity,
        "out-of-range slot = baseline row 0"
    );
}

#[test]
fn heavier_roster_row_falls_faster_through_step() {
    // End-to-end through `step`: two airborne fighters, identical except char_id -> roster row.
    // Row 1 gets doubled gravity; after stepping it must have fallen further. Proves per-fighter
    // physics resolution is wired through the real reducer, not just the accessor.
    let mut t = Tune::default();
    let mut fast = CharSpec::KNEEMAN;
    fast.phys.gravity = 0.34; // 2x KNEEMAN
    std::sync::Arc::make_mut(&mut t.roster)[1] = fast;

    let mut s = SimState::spawn_n(2);
    for f in s.fighters.iter_mut() {
        f.state = CharState::Air;
        f.pos = Vector2::new(600.0, 200.0);
        f.vel = Vector2::ZERO;
        f.ground_plat = -1;
        f.fast_falling = false;
    }
    s.fighters[0].char_id = 0;
    s.fighters[1].char_id = 1;

    let mut cur = s;
    for _ in 0..10 {
        cur = step(&cur, &[&IDLE, &IDLE], &t);
    }
    assert!(
        cur.fighters[1].vel.y > cur.fighters[0].vel.y,
        "the heavier-gravity roster row accelerates downward faster ({} vs {})",
        cur.fighters[1].vel.y,
        cur.fighters[0].vel.y
    );
}

const IDLE: InputFrame = InputFrame {
    dir: 0.0,
    aim_y: 0.0,
    cx: 0.0,
    cy: 0.0,
    jump: false,
    jump_held: false,
    shorthop: false,
    shield_held: false,
    shield_pressed: false,
    down: false,
    down_pressed: false,
    attack: false,
    attack_held: false,
    grab: false,
    special: false,
};

// ── (3) Lucas (row 2, plans/swordsman-lucas.md row 7 "fast-Lucas"): selectable, spawns, and
// distinct from KNEEMAN through the real `step` reducer -- the pattern (1)/(2) above pin, grown
// to the roster's newest row. `char_id` per fighter is all `step` needs (`for_char` internally);
// tests always pass the row-0 flat `Tune::default()` as the sim config and read per-character
// constants (frame windows, reach) off `t.for_char(id)` where a fixture needs them.

#[test]
fn lucas_is_selectable_and_falls_floatier_than_kneeman() {
    // char_id 5 selects Lucas's roster row (the `lucas` shell art slot; the roster is aligned to the
    // shell art order, see chars/mod.rs). A menu pick of 5 resolves Lucas's own floaty CharSpec.
    let t = Tune::default();
    assert_ne!(
        t.for_char(5).gravity,
        t.for_char(0).gravity,
        "Lucas's row (5) resolves his own physics, not KNEEMAN's"
    );

    let mut s = SimState::spawn_n(2);
    for f in s.fighters.iter_mut() {
        f.state = CharState::Air;
        f.pos = Vector2::new(600.0, 200.0);
        f.vel = Vector2::ZERO;
        f.ground_plat = -1;
        f.fast_falling = false;
    }
    s.fighters[0].char_id = 0; // KNEEMAN
    s.fighters[1].char_id = 5; // Lucas

    let mut cur = s;
    for _ in 0..10 {
        cur = step(&cur, &[&IDLE, &IDLE], &t);
    }
    assert!(
        cur.fighters[1].vel.y < cur.fighters[0].vel.y,
        "Lucas's floaty-faller gravity accelerates downward SLOWER than KNEEMAN's ({} vs {})",
        cur.fighters[1].vel.y,
        cur.fighters[0].vel.y
    );
}

#[test]
fn lucas_double_jump_apex_stays_bounded() {
    // Regression: Lucas's airjump_v (3.60) once EXCEEDED his own fullhop_v (3.55) on 0.11 gravity,
    // so a standing fullhop+double-jump cleared ~232 units and flew off the top blast zone ("double
    // jump himself to death"). `airjump_v` resets vel.y (za_warudo.rs), so the max standing ceiling
    // is fullhop_apex + dj_apex. Pin both invariants on the resolved (pixel-unit) physics:
    //   (1) the double jump apex must NOT exceed his own full hop apex, and
    //   (2) his total standing ceiling must stay within a floaty margin of KNEEMAN's -- not 1.6x it.
    let t = Tune::default();
    let apex = |v: f32, g: f32| (v * v) / (2.0 * g.abs()); // v^2/(2g); resolved v is negative (up)

    let lucas = t.for_char(5);
    let knee = t.for_char(0);
    let lucas_fh = apex(lucas.fullhop_v, lucas.gravity);
    let lucas_dj = apex(lucas.airjump_v, lucas.gravity);
    let knee_ceiling = apex(knee.fullhop_v, knee.gravity) + apex(knee.airjump_v, knee.gravity);
    let lucas_ceiling = lucas_fh + lucas_dj;

    assert!(
        lucas_dj <= lucas_fh,
        "Lucas's double jump apex ({lucas_dj:.1}) must not exceed his full hop apex ({lucas_fh:.1})"
    );
    assert!(
        lucas_ceiling < knee_ceiling * 1.25,
        "Lucas's fullhop+DJ ceiling ({lucas_ceiling:.1}) must stay near KNEEMAN's ({knee_ceiling:.1}), not fly off the top"
    );
    assert!(
        lucas_ceiling > knee_ceiling,
        "...but still floatier than KNEEMAN (his identity): {lucas_ceiling:.1} > {knee_ceiling:.1}"
    );
}

#[test]
fn lucas_ftilt_connects_as_a_disjoint_normal() {
    // A normal actually connects through `step`: Lucas's ftilt (a disjoint stick sweep, off.x
    // 96.0 -- past the DUMMY_R 48 hurtbox radius) armed and stepped into a standing victim.
    let t = Tune::default();
    let lucas = t.for_char(5);
    let mut s = SimState::spawn();
    s.fighters[0].char_id = 5;
    s.fighters[0].state = CharState::Ftilt;
    s.fighters[0].frame = lucas.ftilt.boxes[0].start; // the active frame lands this step
    s.fighters[0].pos = Vector2::new(600.0, GROUND_Y);
    s.fighters[0].facing = 1.0;
    s.fighters[0].ground_plat = 0;
    s.fighters[0].arm_hits();
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].pos = Vector2::new(600.0 + 96.0, GROUND_Y); // sits at the tip's off.x
    s.fighters[1].ground_plat = 0;

    let c = step(&s, &[&IDLE, &IDLE], &t);
    assert_eq!(
        c.fighters[1].damage, 10.0,
        "Lucas's ftilt damage lands on the victim"
    );
    assert!(c.fighters[1].hitstun > 0, "the connect launches hitstun");
}

/// A grabber (`char_id`) mid-`Grab`, active window live this step, `gap` px from a standing victim.
fn grab_fixture(t: &Tune, char_id: u8, gap: f32) -> SimState {
    let row = t.for_char(char_id);
    let mut s = SimState::spawn();
    s.fighters[0].char_id = char_id;
    s.fighters[0].state = CharState::Grab;
    s.fighters[0].frame = row.grab_startup; // inside [grab_startup, grab_startup+grab_active)
    s.fighters[0].pos = Vector2::new(600.0, GROUND_Y);
    s.fighters[0].facing = 1.0;
    s.fighters[0].ground_plat = 0;
    s.fighters[1].pos = Vector2::new(600.0 + gap, GROUND_Y);
    s.fighters[1].ground_plat = 0;
    s
}

#[test]
fn lucas_tether_grab_reaches_farther_than_kneeman() {
    // grab_range is the ONLY reach mechanism `resolve_grab` reads (moves/throw.rs) -- this codebase
    // has no Hitbox-shaped "Grab role" to author against. Lucas's tether is that same scalar, more
    // than doubled (230.0 vs KNEEMAN's 100.0): a 250px gap sits within Lucas's catch tolerance
    // (grab_range 230 + victim hurtbox r 48 + GRAB_CATCH_R 36 slop = up to 314px) but beyond
    // KNEEMAN's (100 + 48 + 36 = up to 184px).
    let t = Tune::default();
    assert!(
        t.for_char(5).grab_range > t.for_char(0).grab_range * 2.0,
        "Lucas's grab_range more than doubles KNEEMAN's"
    );

    let s = grab_fixture(&t, 5, 250.0);
    let c = step(&s, &[&IDLE, &IDLE], &t);
    assert_eq!(
        c.fighters[0].grab_link, 1,
        "Lucas's tether catches at a 250px gap"
    );

    let s = grab_fixture(&t, 0, 250.0);
    let c = step(&s, &[&IDLE, &IDLE], &t);
    assert_eq!(
        c.fighters[0].grab_link, -1,
        "the same 250px gap is out of KNEEMAN's reach"
    );
}
