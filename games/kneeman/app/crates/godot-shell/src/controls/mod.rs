//! Controls: the IMPURE half — the ONLY place in the gameplay path that touches a raw device. It
//! reads Godot devices, merges them per player, and hands a [`pad::RawPad`] to the pure [`pad`] core,
//! which does the device-agnostic assembly into the sim's semantic [`InputFrame`]. Everything
//! downstream sees only `InputFrame`; nothing else may name `Input`, `JoyButton`, `JoyAxis`, or an
//! action string. The action universe is [`GameAction`].
//!
//! Lockdown invariant: `Input::singleton` / `JoyButton` / `JoyAxis` / `is_action_*` appear in this
//! file (and `ui/debug.rs`'s pad *readout*) only. The pure `pad.rs` has no Godot at all.
//!
//! P1 keyboard follows the shared V1 semantic adapter: WASD moves, W is jump/tap-up, the arrow
//! keys are the c-stick for aim/attack, Left Command is attack, X is short hop, Space is special, Left Shift
//! is shield, and Right Shift is grab. The same `GameAction` names also cover the existing gamepad
//! and touch equivalents upstream in the fold.
//!
//! Couch co-op: `poll` is player one (keyboard `move_*`/named actions + pad[0] + touch); `poll_p2` is
//! player two, read straight off the SECOND gamepad only -- no keyboard cluster (P1 now owns the
//! whole keyboard). Netplay still supplies its P2 over the wire; `poll_p2` is local-path only.

pub mod pad;
#[path = "0_bindings.rs"]
pub mod bindings;

use std::cell::Cell;

use godot::classes::{
    Input, InputEvent, InputEventScreenDrag, InputEventScreenTouch,
};
use godot::global::{JoyButton, Key};
use godot::prelude::*;

use crate::input_frame::InputFrame;
use pad::{PadMemory, RawPad};

const STICK_DEADZONE: f32 = 0.22; // pad stick magnitude below this reads as neutral

thread_local! {
    // Per-player tap-jump memory, owned here and threaded through the pure fold each frame.
    static P1_MEM: Cell<PadMemory> = Cell::new(PadMemory::default());
    static P2_MEM: Cell<PadMemory> = Cell::new(PadMemory::default());
    // P1's "down" edge tracker: the merged move axis (S / stick / touch) has no just-pressed of its
    // own either, now that keyboard down no longer rides the `ui_down` action's built-in edge.
    static P1_DOWN_PREV: Cell<bool> = const { Cell::new(false) };
    // Player two's raw-button edge tracker (held -> pressed); stays impure (per-device history).
    static P2_PREV_MASK: Cell<u8> = const { Cell::new(0) };
}

/// Release gameplay actions and edge memory on focus changes. C-stick keys now use actions too;
/// no synthetic event dispatch is needed from inside the node's notification callback.
pub fn release_all() {
    let mut input = Input::singleton();
    for a in [
        GameAction::Jump,
        GameAction::ShortHop,
        GameAction::Attack,
        GameAction::Shield,
        GameAction::Grab,
        GameAction::Special,
        GameAction::Down,
    ] {
        for name in a.names() {
            input.action_release(*name);
        }
    }
    for row in P1_MANUAL {
        for name in row.keyboard { input.action_release(*name); }
    }
    P1_MEM.set(PadMemory::default());
    P2_MEM.set(PadMemory::default());
    for &(p1, p2) in bindings::PAD_ACTIONS.iter().chain(bindings::PAD_STICKS) {
        input.action_release(p1);
        input.action_release(p2);
    }
    P1_DOWN_PREV.set(false);
    P2_PREV_MASK.set(0);
}

/// The action universe. Every game-meaningful input is one of these; nothing downstream names a
/// physical key or pad button. `names` lists the `project.godot` action(s) each maps to (multiple =
/// aliases OR'd together). This is the single source for action strings, incl. the touch buttons.
#[derive(Clone, Copy)]
pub enum GameAction {
    Jump,
    ShortHop,
    Attack,
    Shield,
    Grab,
    Special,
    Down,
}

impl GameAction {
    pub fn names(self) -> &'static [&'static str] {
        match self {
            // Jump is the shared semantic lane for W / pad A / touch jump; keep it free of the
            // old menu aliases so the c-stick and special button stay separate.
            GameAction::Jump => &["jump"],
            GameAction::ShortHop => &["shorthop"],
            GameAction::Attack => &["attack"],
            GameAction::Shield => &["shield"],
            GameAction::Grab => &["grab"],
            GameAction::Special => &["special"],
            // Down is derived from the merged movement axis (S / stick / touch) instead of a
            // separate named action so the same fast-fall lane works on every device.
            GameAction::Down => &[],
        }
    }
}

/// One row of the pause-menu control manual: an action label plus how player one reaches it on
/// keyboard and gamepad. Keyboard entries are live InputMap action names. Pad descriptions still
/// describe the fixed raw-device adapter below.
pub struct ManualRow {
    pub action: &'static str,
    pub keyboard: &'static [&'static str],
    pub gamepad: &'static str,
}

/// Player one's manual: keyboard + gamepad binding per action, for the pause menu's first page.
/// Sourced from the shared V1 input table and the manual axis/raw-key reads in `poll` above.
/// Cells are short key/button tokens only (no prose) so the pause-menu table stays compact.
pub const P1_MANUAL: &[ManualRow] = &[
    ManualRow {
        action: "Move",
        keyboard: &["move_up", "move_left", "move_down", "move_right"],
        gamepad: "L-stick / D-pad",
    },
    ManualRow {
        action: "Jump*",
        keyboard: &["jump"],
        gamepad: "A",
    },
    ManualRow {
        action: "Short hop",
        keyboard: &["shorthop"],
        gamepad: "R1",
    },
    ManualRow {
        action: "Attack / pick up",
        keyboard: &["attack"],
        gamepad: "X / R2",
    },
    ManualRow {
        action: "Special",
        keyboard: &["special"],
        gamepad: "B",
    },
    ManualRow {
        action: "Grab / throw",
        keyboard: &["grab"],
        gamepad: "Y/Back",
    },
    ManualRow {
        action: "Shield/dodge",
        keyboard: &["shield"],
        gamepad: "L1",
    },
    ManualRow {
        action: "Fast-fall",
        keyboard: &[], // Same move-down binding; avoid a duplicate persistence row.
        gamepad: "D-pad Down",
    },
    ManualRow {
        action: "C-stick (aim/attack)",
        keyboard: &["aim_up", "aim_left", "aim_down", "aim_right"],
        gamepad: "R-stick",
    },
    ManualRow {
        action: "Pause",
        keyboard: &[], // Menu navigation remains fixed in this checkpoint.
        gamepad: "Start",
    },
];

/// True if any alias of `a` is currently held.
fn held(input: &mut Input, a: GameAction) -> bool {
    a.names().iter().any(|n| input.is_action_pressed(*n))
}

/// True if any alias of `a` had its rising edge this frame.
fn pressed(input: &mut Input, a: GameAction) -> bool {
    a.names().iter().any(|n| input.is_action_just_pressed(*n))
}

/// Capture a semantic V1 button edge directly from Godot's event queue. The fixed-step sampler
/// still owns held levels and axes; shells use this only so a very short keyboard/gamepad tap cannot
/// begin and end between two simulation ticks.
pub fn event_pressed(event: &Gd<InputEvent>, action: GameAction) -> bool {
    action
        .names()
        .iter()
        .any(|name| event.is_action_pressed(*name))
}

/// Compact keyboard legend generated from the same rows as V1's pause-menu manual.
pub fn keyboard_manual() -> String {
    P1_MANUAL
        .iter()
        .filter(|row| row.action != "Pause")
        .enumerate()
        .fold(String::new(), |mut text, (index, row)| {
            if index > 0 {
                text.push_str(if index == 5 { "\n" } else { " · " });
            }
            text.push_str(&row.keyboard.iter().map(|name| bindings::label(name)).collect::<Vec<_>>().join("/"));
            text.push(' ');
            text.push_str(row.action);
            text
        })
}

// --- device seam for the touch UI (kneeman owns the layout; the raw device stays here) ---
// These keep `Input`, `InputEventScreen*`, and `Key` out of kneeman.rs: the on-screen pad reads its
// layout there but routes every raw-device call through this file, so the lockdown invariant holds
// (enforced by .dl/lint-input-device.dl).

/// A screen touch, classified out of the raw godot `InputEvent` so the caller names no device type.
/// `pos` is screen-space; `finger` is the touch index.
pub enum Touch {
    Down { finger: i64, pos: Vector2 },
    Up { finger: i64 },
    Drag { finger: i64, pos: Vector2 },
}

/// Classify a raw input event as a screen touch, or `None` if it is something else.
pub fn classify_touch(event: &Gd<InputEvent>) -> Option<Touch> {
    if let Ok(t) = event.clone().try_cast::<InputEventScreenTouch>() {
        let finger = t.get_index() as i64;
        return Some(if t.is_pressed() {
            Touch::Down {
                finger,
                pos: t.get_position(),
            }
        } else {
            Touch::Up { finger }
        });
    }
    if let Ok(d) = event.clone().try_cast::<InputEventScreenDrag>() {
        return Some(Touch::Drag {
            finger: d.get_index() as i64,
            pos: d.get_position(),
        });
    }
    None
}

/// Press every named action (the touch buttons synthesize the same actions the keyboard binds).
pub fn press_actions(actions: &[&str]) {
    let mut input = Input::singleton();
    for a in actions {
        input.action_press(*a);
    }
}

/// Release every named action held by a lifted finger.
pub fn release_actions(actions: &[&str]) {
    let mut input = Input::singleton();
    for a in actions {
        input.action_release(*a);
    }
}

/// True if at least one gamepad is connected (the touch pad hides itself when one is).
pub fn gamepad_connected() -> bool {
    !Input::singleton().get_connected_joypads().is_empty()
}

/// Debug test-spawn: the rising edges of number keys 1..=0 (ten slots) since `prev`. Returns the slot
/// indices that went down THIS call and updates `prev`. Local-only; keeps `Key` out of the caller.
pub fn number_key_edges(prev: &mut [bool; 10]) -> Vec<usize> {
    const KEYS: [Key; 10] = [
        Key::KEY_1,
        Key::KEY_2,
        Key::KEY_3,
        Key::KEY_4,
        Key::KEY_5,
        Key::KEY_6,
        Key::KEY_7,
        Key::KEY_8,
        Key::KEY_9,
        Key::KEY_0,
    ];
    let input = Input::singleton();
    let mut edges = Vec::new();
    for (i, key) in KEYS.iter().enumerate() {
        let down = input.is_key_pressed(*key);
        if down && !prev[i] {
            edges.push(i);
        }
        prev[i] = down;
    }
    edges
}

/// Read every game action for the local player into the sim's semantic [`InputFrame`]. `touch_stick`
/// and `touch_cstick` are the on-screen sticks' (x, y) in [-1, 1] (the mobile UI owns those
/// widgets; we just merge them in).
// reuse-kit-library(semantic-input): shared device->InputFrame boundary (keyboard+gamepad+touch merge) consumed by both V1 (kneeman) and V4 (v4_game feeds V4NativeControls).
pub fn poll(touch_stick: (f32, f32), touch_cstick: (f32, f32)) -> InputFrame {
    let mut input = Input::singleton();
    // Keyboard movement: WASD, bound to the `move_*` actions in project.godot (NOT the built-in
    // `ui_*` actions -- those stay on the arrows so menu navigation is untouched).
    let mut dir = input.get_axis("move_left", "move_right");
    let mut aim_y = input.get_axis("move_up", "move_down"); // -1 up .. +1 down
    let mut pad_down = false;
    // Keyboard c-stick: remappable actions folded into a unit-deflection vector by the
    // pure core. The adapter keeps aim/attack on the same semantic lane everywhere, including
    // gamepad and touch equivalents.
    let (mut c_x, mut c_y) = pad::dpad_to_cstick(
        input.is_action_pressed("aim_up"),
        input.is_action_pressed("aim_down"),
        input.is_action_pressed("aim_left"),
        input.is_action_pressed("aim_right"),
    );
    // Pad-only actions preserve keyboard priority and raw analog magnitudes. D-pad still
    // overrides the movement stick; the existing deadzones apply after sampling.
    if let Some(dev) = input.get_connected_joypads().get(0) {
        let dev = dev as i32;
        let dz = 0.2;
        let sx = input.get_action_raw_strength("pad_right") - input.get_action_raw_strength("pad_left");
        let sy = input.get_action_raw_strength("pad_down") - input.get_action_raw_strength("pad_up");
        // right stick = the c-stick (smash / aerial macro); deadzone like the move stick. Keyboard
        // arrows take priority (already folded above); the pad only fills in when they're neutral.
        let rx = input.get_action_raw_strength("pad_aim_right") - input.get_action_raw_strength("pad_aim_left");
        let ry = input.get_action_raw_strength("pad_aim_down") - input.get_action_raw_strength("pad_aim_up");
        if c_x == 0.0 && c_y == 0.0 && (rx * rx + ry * ry).sqrt() > dz {
            c_x = rx;
            c_y = ry;
        }
        let dpx = input.is_joy_button_pressed(dev, JoyButton::DPAD_RIGHT) as i32 as f32
            - input.is_joy_button_pressed(dev, JoyButton::DPAD_LEFT) as i32 as f32;
        let dpy = input.is_joy_button_pressed(dev, JoyButton::DPAD_DOWN) as i32 as f32
            - input.is_joy_button_pressed(dev, JoyButton::DPAD_UP) as i32 as f32;
        let px = if dpx != 0.0 {
            dpx
        } else if sx.abs() > dz {
            sx
        } else {
            0.0
        };
        let py = if dpy != 0.0 {
            dpy
        } else if sy.abs() > dz {
            sy
        } else {
            0.0
        };
        if dir == 0.0 {
            dir = px;
        }
        if aim_y == 0.0 {
            aim_y = py;
        }
        pad_down = py > 0.4;
    }
    // On-screen touch stick (mobile). Lowest priority: only fills axes the keyboard/pad left at 0.
    let (tsx, tsy) = touch_stick;
    if dir == 0.0 && tsx.abs() > STICK_DEADZONE {
        dir = tsx;
    }
    if aim_y == 0.0 && tsy.abs() > STICK_DEADZONE {
        aim_y = tsy;
    }
    if tsy > 0.4 {
        pad_down = true;
    }
    // On-screen touch c-stick: same lowest-priority fill as the move stick above (keyboard arrows,
    // then the pad's right stick, already claimed c_x/c_y if live).
    let (tcx, tcy) = touch_cstick;
    if c_x == 0.0 && c_y == 0.0 && (tcx * tcx + tcy * tcy).sqrt() > STICK_DEADZONE {
        c_x = tcx;
        c_y = tcy;
    }
    // Keyboard "down" (fast-fall / crouch / soft-platform drop): folded from the merged move axis
    // (S) instead of a dedicated `ui_down` alias so it behaves the same across keyboard, pad, and
    // touch. `down_edge` tracks the held->true transition by hand (prev-frame Cell), same pattern
    // as the R2 trigger below, so `down_pressed` still fires a clean edge for keyboard.
    let down_flag = aim_y > 0.4;
    if down_flag {
        pad_down = true;
    }
    let down_edge = down_flag && !P1_DOWN_PREV.get();
    P1_DOWN_PREV.set(down_flag);
    // Hand the merged device reads to the pure core; it owns tap-jump (flick the stick up to jump,
    // no button) and the frame assembly, so behavior is identical across touch/pad/keyboard. Jump
    // also gets a dedicated button now (W, aliased into the "jump" action alongside pad button A --
    // see project.godot), so `held`/`pressed(Jump)` already carries the keyboard: holding W keeps
    // `jump_held` true for the full-hop/short-hop release window, same as holding the pad button.
    let raw = RawPad {
        move_x: dir,
        move_y: aim_y,
        c_x,
        c_y,
        jump_held: held(&mut input, GameAction::Jump),
        jump_pressed: pressed(&mut input, GameAction::Jump),
        shorthop_pressed: pressed(&mut input, GameAction::ShortHop),
        shield_held: held(&mut input, GameAction::Shield),
        shield_pressed: pressed(&mut input, GameAction::Shield),
        down_held: held(&mut input, GameAction::Down) || pad_down,
        down_pressed: pressed(&mut input, GameAction::Down) || down_edge,
        attack_held: held(&mut input, GameAction::Attack),
        attack_pressed: pressed(&mut input, GameAction::Attack),
        grab_pressed: pressed(&mut input, GameAction::Grab),
        special_pressed: pressed(&mut input, GameAction::Special),
    };
    let mut mem = P1_MEM.get();
    let frame = mem.frame(&raw);
    P1_MEM.set(mem);
    frame
}

// --- player two (couch co-op) ------------------------------------------------------------------
// P2 = the SECOND connected gamepad ONLY. P1 owns the keyboard through the shared semantic
// adapter, so couch co-op is gamepad+gamepad. Device-scoped InputMap actions own buttons;
// movement/c-stick axes retain the raw-device path below.
// Bits in the held mask, for edge detection across frames:
const B_JUMP: u8 = 1 << 0;
const B_SHORTHOP: u8 = 1 << 1;
const B_ATTACK: u8 = 1 << 2;
const B_SHIELD: u8 = 1 << 3;
const B_GRAB: u8 = 1 << 4;
const B_SPECIAL: u8 = 1 << 5;
const B_DOWN: u8 = 1 << 6;

/// P2 device-scoped InputMap action names.
fn p2_pad_action(a: GameAction) -> &'static str {
    match a {
        GameAction::Jump => "p2_jump",
        GameAction::ShortHop => "p2_shorthop",
        GameAction::Attack => "p2_attack",
        GameAction::Shield => "p2_shield",
        GameAction::Grab => "p2_grab",
        GameAction::Special => "p2_special",
        GameAction::Down => "",
    }
}

#[test]
fn p2_actions_match_the_binding_registry() {
    let actions = [GameAction::Jump, GameAction::ShortHop, GameAction::Attack,
        GameAction::Shield, GameAction::Grab, GameAction::Special];
    assert_eq!(actions.map(p2_pad_action).as_slice(), bindings::PAD_ACTIONS.iter().map(|r| r.1).collect::<Vec<_>>());
}

/// Player two's frame for local two-player: the SECOND connected gamepad, all-neutral when it isn't
/// there so the caller feeds it every frame for free (couch co-op "turns on" the moment someone grabs
/// gamepad 2). Netplay does NOT use this (its P2 arrives over the wire).
pub fn poll_p2() -> InputFrame {
    let mut input = Input::singleton();
    let pad2 = input.get_connected_joypads().get(1).map(|d| d as i32);

    let mut dir = 0.0;
    let mut aim_y = 0.0;
    let mut c_x = 0.0;
    let mut c_y = 0.0;
    if let Some(dev) = pad2 {
        let sx = input.get_action_raw_strength("p2_pad_right") - input.get_action_raw_strength("p2_pad_left");
        let sy = input.get_action_raw_strength("p2_pad_down") - input.get_action_raw_strength("p2_pad_up");
        let rx = input.get_action_raw_strength("p2_pad_aim_right") - input.get_action_raw_strength("p2_pad_aim_left");
        let ry = input.get_action_raw_strength("p2_pad_aim_down") - input.get_action_raw_strength("p2_pad_aim_up");
        if (rx * rx + ry * ry).sqrt() > STICK_DEADZONE {
            c_x = rx;
            c_y = ry;
        }
        let dpx = input.is_joy_button_pressed(dev, JoyButton::DPAD_RIGHT) as i32 as f32
            - input.is_joy_button_pressed(dev, JoyButton::DPAD_LEFT) as i32 as f32;
        let dpy = input.is_joy_button_pressed(dev, JoyButton::DPAD_DOWN) as i32 as f32
            - input.is_joy_button_pressed(dev, JoyButton::DPAD_UP) as i32 as f32;
        dir = if dpx != 0.0 {
            dpx
        } else if sx.abs() > STICK_DEADZONE {
            sx
        } else {
            0.0
        };
        aim_y = if dpy != 0.0 {
            dpy
        } else if sy.abs() > STICK_DEADZONE {
            sy
        } else {
            0.0
        };
    }

    // held mask: a button is "held" if its pad[1] button is down this frame.
    let mut mask = 0u8;
    for (a, bit) in [
        (GameAction::Jump, B_JUMP),
        (GameAction::ShortHop, B_SHORTHOP),
        (GameAction::Attack, B_ATTACK),
        (GameAction::Shield, B_SHIELD),
        (GameAction::Grab, B_GRAB),
        (GameAction::Special, B_SPECIAL),
    ] {
        let down = pad2.is_some() && input.is_action_pressed(p2_pad_action(a));
        if down {
            mask |= bit;
        }
    }
    if aim_y > 0.4 {
        mask |= B_DOWN;
    }

    let prev = P2_PREV_MASK.get();
    P2_PREV_MASK.set(mask);
    let edge = |bit: u8| (mask & bit != 0) && (prev & bit == 0);
    let held = |bit: u8| mask & bit != 0;

    // Same pure core as P1 (tap-jump + assembly); only the raw reads + edge detection differ here.
    let raw = RawPad {
        move_x: dir,
        move_y: aim_y,
        c_x,
        c_y,
        jump_held: held(B_JUMP),
        jump_pressed: edge(B_JUMP),
        shorthop_pressed: edge(B_SHORTHOP),
        shield_held: held(B_SHIELD),
        shield_pressed: edge(B_SHIELD),
        down_held: held(B_DOWN),
        down_pressed: edge(B_DOWN),
        attack_held: held(B_ATTACK),
        attack_pressed: edge(B_ATTACK),
        grab_pressed: edge(B_GRAB),
        special_pressed: edge(B_SPECIAL),
    };
    let mut mem = P2_MEM.get();
    let frame = mem.frame(&raw);
    P2_MEM.set(mem);
    frame
}
