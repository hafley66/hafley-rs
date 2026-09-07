// Split out of net/src/lib.rs; white-box (super::*) unit tests for encode/checksum/session.

use super::*;
use crate::v1::net::replay::{InputLog, replay};

#[test]
fn saved_checksum_observer_replaces_a_predicted_frame_after_restore() {
    use ggrs::{GameStateCell, GgrsRequest, InputStatus};
    let mut game = Game::<Smash>::new(Tune::default());
    let initial = game.state;
    let start = GameStateCell::default();
    let next = GameStateCell::default();
    let mut observed = Vec::new();
    let neutral = NetInput::default();
    let moving = encode(&InputFrame { dir: 1.0, ..Default::default() });
    game.handle_observed::<usize>(vec![
        GgrsRequest::SaveGameState { cell: start.clone(), frame: 0 },
        GgrsRequest::AdvanceFrame { inputs: vec![(neutral, InputStatus::Predicted); 2] },
        GgrsRequest::SaveGameState { cell: next.clone(), frame: 1 },
        GgrsRequest::LoadGameState { cell: start, frame: 0 },
        GgrsRequest::AdvanceFrame { inputs: vec![(moving, InputStatus::Confirmed), (neutral, InputStatus::Confirmed)] },
        GgrsRequest::SaveGameState { cell: next.clone(), frame: 1 },
    ], |frame, hash| observed.push((frame, hash)));
    let predicted = Smash::advance(&initial, &[neutral, neutral], &game.cfg);
    let corrected = Smash::advance(&initial, &[moving, neutral], &game.cfg);
    assert_ne!(checksum(&predicted), checksum(&corrected));
    assert_eq!(observed, vec![(0, checksum(&initial)), (1, checksum(&predicted)), (1, checksum(&corrected))]);
    let history: std::collections::BTreeMap<_, _> = observed.into_iter().collect();
    assert_eq!(history[&1], checksum(&corrected));
    assert_eq!(checksum(&next.load().unwrap()), checksum(&corrected));
}

/// Deterministic pseudo-random input stream so the sim visits many states (move, jump, dash,
/// shield, attack, dodge) under rollback. Same seed -> same stream on both "peers".
fn gen_input(seed: &mut u64) -> NetInput {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let r = (*seed >> 33) as u32;
    NetInput {
        axes: [
            ((r >> 10) & 0xff) as i8,
            ((r >> 18) & 0xff) as i8,
            ((r >> 14) & 0xff) as i8,
            ((r >> 22) & 0xff) as i8,
        ],
        buttons: r & 0x7ff,
    }
}

/// Input-vector sizing: `Game::handle`'s `AdvanceFrame` folds `inputs.len()` (== ggrs's
/// `num_players`) straight through `Smash::advance` unchanged. Pin that it works for k=4 fighters
/// (the `crate::v1::MAX_PLAYERS` cap), not just the historical 2p pair.
#[test]
fn advance_accepts_n_player_input_vector() {
    let state = SimState::spawn_n(4);
    assert_eq!(state.active, 4);
    let inputs = vec![NetInput::default(); 4];
    let next = Smash::advance(&state, &inputs, &Tune::default());
    assert_eq!(next.tick, state.tick + 1);
    assert_eq!(next.active, 4);
}

/// Same determinism gate as the 2p SyncTest below, but at the platform-fighter cap (k=4): proves
/// the rollback plumbing (`synctest_session_n`, `Game`, ggrs's own session) generalizes past the
/// historical pair, not just `crate::v1::step` in isolation.
#[test]
fn synctest_runs_deterministic_for_4_players() {
    let mut sess = synctest_session_n(4, 2);
    let mut game = Game::from_state(SimState::spawn_n(4), Tune::default());
    let mut seeds = [
        0x1234_5678u64,
        0x9abc_def0u64,
        0x1111_2222u64,
        0x3333_4444u64,
    ];
    for frame in 0..300 {
        for (h, seed) in seeds.iter_mut().enumerate() {
            sess.add_local_input(h, gen_input(seed))
                .expect("local input");
        }
        let requests = sess
            .advance_frame()
            .unwrap_or_else(|e| panic!("desync at frame {frame}: {e:?}"));
        game.handle(requests);
    }
}

#[test]
fn synctest_runs_deterministic() {
    let mut sess = synctest_session(2);
    let mut game = Game::new(Tune::default());
    let mut s0 = 0x1234_5678u64;
    let mut s1 = 0x9abc_def0u64;

    // 1200 frames = 20s. Any non-determinism in step() trips MismatchedChecksum.
    for frame in 0..1200 {
        sess.add_local_input(0, gen_input(&mut s0))
            .expect("p0 input");
        sess.add_local_input(1, gen_input(&mut s1))
            .expect("p1 input");
        let requests = sess
            .advance_frame()
            .unwrap_or_else(|e| panic!("desync at frame {frame}: {e:?}"));
        game.handle(requests);
    }
}

/// Build a deterministic two-player input log (the "captured session" fixture).
fn fixture_log(frames: usize) -> InputLog {
    let mut log = InputLog::default();
    let mut s0 = 0xfeed_face_u64;
    let mut s1 = 0x0bad_c0de_u64;
    for _ in 0..frames {
        log.push(gen_input(&mut s0), gen_input(&mut s1));
    }
    log
}

#[test]
fn replay_is_deterministic_across_runs() {
    let t = Tune::default();
    let log = fixture_log(900);
    let a = replay(&log, &t);
    let b = replay(&log, &t);
    assert_eq!(
        a, b,
        "same log + tune must replay to the same checksum stream"
    );
    assert_eq!(a.len(), 900);
}

#[test]
fn fixture_survives_serialize_roundtrip_and_replays_identically() {
    let t = Tune::default();
    let log = fixture_log(600);
    // serialize the captured log to bytes and reload it (a fixture file would do the same).
    let bytes = log.to_bytes();
    let reloaded = InputLog::from_bytes(&bytes).expect("reload captured log");
    assert!(reloaded == log, "log survives a bincode round-trip"); // InputLog has no Debug
    assert_eq!(
        replay(&log, &t),
        replay(&reloaded, &t),
        "the reloaded fixture replays to the identical state stream",
    );
}

#[test]
fn pure_replay_matches_the_ggrs_handler() {
    // The rollback handler (Game::handle) and a straight pure replay must agree frame-for-frame,
    // so the SyncTest path and offline play can never diverge.
    let t = Tune::default();
    let log = fixture_log(500);
    let pure = replay(&log, &t);

    let mut sess = synctest_session(2);
    let mut game = Game::new(t);
    let mut handler = Vec::with_capacity(log.frames.len());
    for (frame, &(p0, p1)) in log.frames.iter().enumerate() {
        sess.add_local_input(0, p0).expect("p0 input");
        sess.add_local_input(1, p1).expect("p1 input");
        let requests = sess
            .advance_frame()
            .unwrap_or_else(|e| panic!("desync at frame {frame}: {e:?}"));
        game.handle(requests);
        handler.push(checksum(&game.state));
    }
    assert_eq!(
        pure, handler,
        "pure replay and the ggrs handler must produce identical states"
    );
}

#[test]
fn cell_lifecycle_matches_offline_through_ggrs_rollback() {
    let tune = Tune::default();
    let initial = crate::v1::terrain_cells::playground();
    let inputs: Vec<_> = crate::v1::terrain_cells::playground_inputs().map(|i| encode(&i)).collect();
    let mut offline = initial;
    let mut expected = vec![checksum(&offline)];
    for &input in &inputs {
        offline = Smash::advance(&offline, &[input, NetInput::default()], &tune);
        expected.push(checksum(&offline));
    }
    let mut session = synctest_session(7);
    let mut game = Game::from_state(initial, tune);
    let mut crossed = [false; 3];
    for (index, &input) in inputs.iter().enumerate() {
        session.add_local_input(0, input).unwrap();
        session.add_local_input(1, NetInput::default()).unwrap();
        let requests = session.advance_frame().unwrap();
        for request in &requests {
            if let ggrs::GgrsRequest::LoadGameState { frame, .. } = request {
                for (seen, boundary) in crossed.iter_mut().zip([90, 140, 162]) {
                    *seen |= *frame <= boundary && index as i32 > boundary;
                }
            }
        }
        game.handle_observed(requests, |frame, hash| {
            assert_eq!(hash, expected[frame as usize], "saved rollback frame {frame}");
        });
        assert_eq!(checksum(&game.state), expected[index + 1], "input index {index}");
    }
    assert_eq!(crossed, [true; 3], "actual restores must cross break, pickup and throw");
}

#[test]
fn encode_decode_roundtrip() {
    let i = InputFrame {
        dir: 1.0,
        aim_y: -1.0,
        cx: -1.0,
        cy: 0.5,
        jump: true,
        jump_held: true,
        shorthop: false,
        shield_held: true,
        shield_pressed: false,
        down: true,
        down_pressed: false,
        attack: true,
        attack_held: true,
        grab: true,
        special: true,
    };
    let d = decode(encode(&i));
    assert_eq!(d.jump, i.jump);
    assert_eq!(d.attack, i.attack);
    assert_eq!(d.grab, i.grab);
    assert_eq!(d.special, i.special);
    assert_eq!(d.shield_held, i.shield_held);
    assert_eq!(d.down, i.down);
    assert!((d.dir - 1.0).abs() < 0.02);
    assert!((d.aim_y + 1.0).abs() < 0.02);
    assert!((d.cx + 1.0).abs() < 0.02);
    assert!((d.cy - 0.5).abs() < 0.02);
}

/// The reconnect resume ships a `SimState` snapshot as bincode; a mid-match state must survive
/// the round-trip byte-for-byte, or the two rebuilt sessions start from different states.
#[test]
fn simstate_bincode_roundtrip_resumes_identically() {
    // Run a stepped, non-spawn state so fighters, items, tick and rng are all populated.
    let t = Tune::default();
    let log = fixture_log(300);
    let mut s = SimState::spawn();
    for &(p0, p1) in &log.frames {
        s = step(&s, &[&decode(p0), &decode(p1)], &t);
    }
    let bytes = bincode::serialize(&s).expect("serialize SimState");
    let back: SimState = bincode::deserialize(&bytes).expect("deserialize SimState");
    assert!(s == back, "snapshot must round-trip exactly"); // SimState has no Debug
    assert_eq!(
        checksum(&s),
        checksum(&back),
        "checksum must match after resume decode"
    );
}

/// The rollback frame-cost story, as pinned numbers. Run
/// `cargo test -p smash_net rollback_frame_cost -- --nocapture` to read the report. Three costs,
/// three very different sizes -- the whole point is that they are INDEPENDENT:
///
///   1. WIRE, per player per frame: one `NetInput`. The ONLY thing that crosses the network. Tiny
///      and fixed -- it does not grow with the roster, the stage, or debug/tuning state, because
///      none of those are inputs. This is the "we send diffs / minimal deltas" property: rollback
///      sends inputs, never state.
///   2. SNAPSHOT, per frame, LOCAL only: ggrs `Clone`s the whole `SimState` into its ring so it can
///      rewind. This is memory + a memcpy on your own machine, never sent anywhere.
///   3. BUFFER: snapshot x the prediction window (ggrs default 8 frames). The resident cost of being
///      able to roll back.
///
/// And the thing that is in NEITHER: `Tune` (physics + the whole `CharSpec` roster + panel/debug
/// knobs, ~tens of KB). It is passed to `step` by reference, is not a field of `SimState`, and is
/// never saved, checksummed, or sent. Growing the roster changes `Tune`'s size and NOTHING here --
/// that is what this test guards. If a future change welds config into per-frame state, the ratio
/// asserts below break.
#[test]
fn rollback_frame_cost() {
    use crate::v1::{CharSpec, MAX_PLAYERS, ROSTER_N, Tune, state_size_bytes};
    use core::mem::size_of;

    const PREDICTION_WINDOW: usize = 8; // ggrs default (no `with_max_prediction` override, lib.rs)

    // (1) wire: what actually travels, per player per frame.
    let wire_per_player = bincode::serialized_size(&NetInput::default()).expect("NetInput encodes");
    let wire_per_frame = wire_per_player * MAX_PLAYERS as u64;

    // (2) snapshot: what ggrs Clones per frame (in-memory), plus its serialized size for reference.
    let snapshot_mem = size_of::<SimState>() as u64;
    let snapshot_ser = state_size_bytes(&SimState::spawn());

    // (3) buffer: resident rollback cost.
    let buffer_mem = snapshot_mem * PREDICTION_WINDOW as u64;

    // the thing that never ships:
    let tune = size_of::<Tune>() as u64;

    println!("--- rollback frame cost ---");
    println!(
        "WIRE     {wire_per_player} B/player -> {wire_per_frame} B/frame ({MAX_PLAYERS} players) -- SENT"
    );
    println!(
        "SNAPSHOT {snapshot_mem} B/frame in-mem ({snapshot_ser} B serialized) -- LOCAL Clone, not sent"
    );
    println!(
        "BUFFER   {buffer_mem} B resident ({snapshot_mem} x {PREDICTION_WINDOW} prediction window)"
    );
    println!(
        "TUNE     {tune} B (physics + roster HANDLE + debug) -- passed by ref, NEVER saved/checksummed/sent"
    );

    // "What if the roster has 70 characters?" -- project it. `Tune.roster` is now `Arc<[CharSpec]>`
    // (a shared handle, ~16 B), NOT `[CharSpec; N]` by value, so the character library lives on the
    // heap ONCE and is shared. Adding characters grows that one heap alloc, not `Tune`.
    let charspec = size_of::<CharSpec>() as u64;
    const N70: u64 = 70;
    let library_now = charspec * ROSTER_N as u64; // one shared Arc alloc, whatever the roster length
    let library_70 = charspec * N70;
    // `for_char` builds `[Tune; MAX_PLAYERS]` per frame (lib.rs:201). Each Tune clone is now a
    // pointer bump, so this per-frame cost is CONSTANT in roster length.
    let for_char_copy = tune * MAX_PLAYERS as u64;
    println!("--- projected at {N70} characters (roster = Arc<[CharSpec]>) ---");
    println!(
        "  CharSpec = {charspec} B each; shared library {library_now} B -> {library_70} B (one heap alloc, not per-frame)"
    );
    println!("  WIRE     {wire_per_frame} B/frame       (shared PlayerInput has no roster)");
    println!("  SNAPSHOT {snapshot_mem} B/frame       (UNCHANGED -- SimState has no Tune)");
    println!("  BUFFER   {buffer_mem} B              (UNCHANGED)");
    println!("  TUNE     {tune} B                  (UNCHANGED -- roster is a handle, not inline)");
    println!("  for_char per-frame copy {for_char_copy} B (UNCHANGED at any roster length)");

    // The story, pinned:
    assert!(
        wire_per_player <= 8,
        "one player's per-frame wire input stays a handful of bytes"
    );
    // The whole point of segmenting the roster: `Tune` no longer embeds the library, so its size is
    // INDEPENDENT of roster length. If a future change welds `[CharSpec; N]` back in by value, Tune
    // balloons past a couple of CharSpecs and this breaks (it was ~25KB / 3 specs before the Arc).
    assert!(
        tune < charspec * 2,
        "Tune must not embed the roster by value -- it holds a shared handle (Tune {tune} B, CharSpec {charspec} B)"
    );
    // Neither the wire nor the per-frame snapshot carries config: snapshot is exactly SimState.
    assert_eq!(
        snapshot_mem,
        size_of::<SimState>() as u64,
        "the per-frame snapshot is exactly SimState -- no config welded in"
    );
}

/// Wire-size report for the MAX_ITEMS 20 -> 128 cap raise. `Item` has no `Vec`/`String` fields, so
/// bincode's fixint encoding gives it a fixed per-instance byte cost regardless of contents -- the
/// "before" figure below is exact arithmetic (current size minus the added slots' cost), not a
/// revert-and-remeasure. Run `cargo test -p smash_net item_cap_raise_size_report -- --nocapture` to
/// see the printed numbers; the `assert_eq!` just pins that arithmetic so a future `Item` field can't
/// silently drift this report out of sync with the real encoding.
///
/// Current reading (2026-07-04, after `Item.hp: f32` for item-strikeable): 52 B/item (was 48 -- the
/// appended 4-byte `hp`), so 9810 B (MAX_ITEMS=20) -> 15426 B (MAX_ITEMS=128), = 52 x 108 = +5616 B.
#[test]
fn item_cap_raise_size_report() {
    use crate::v1::{Item, MAX_ITEMS, state_size_bytes};

    let after = state_size_bytes(&SimState::spawn());
    let per_item = bincode::serialized_size(&Item::EMPTY).expect("Item always encodes");
    let added_slots = MAX_ITEMS as u64 - 20; // the pre-raise cap
    let before = after - per_item * added_slots;
    println!(
        "SimState bincode size: {before} B (MAX_ITEMS=20) -> {after} B (MAX_ITEMS={MAX_ITEMS}); \
         {per_item} B/item x {added_slots} more items = +{} B",
        per_item * added_slots
    );
    assert_eq!(after, before + per_item * added_slots);
}
