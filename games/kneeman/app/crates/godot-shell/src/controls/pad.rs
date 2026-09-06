//! Pure controls core: device-agnostic, NO Godot. A frame of raw inputs (`RawPad`) plus a tiny
//! per-player memory fold into the sim's semantic [`InputFrame`]. Tap-jump and frame assembly live
//! here so they behave identically across keyboard/pad/touch and are testable without a device.
//!
//! Reusable: lift this file into another prototype as-is. The only game-specific thing it touches is
//! `InputFrame` (the input contract) — swap that and the mapping body for a new game; the RawPad +
//! cross-frame-edge pattern carries over unchanged. This is the "pure" half the impure `mod.rs`
//! (Godot device reads) feeds. See plans/controls-as-crate.md.

use crate::input_frame::InputFrame;

// Tap-jump edge thresholds are tuned for gamepad left-stick/D-pad up flicks, including diagonal
// top-left inputs where X remains negative while Y crosses hard up.
const TAP_JUMP_PREV_UP: f32 = -0.5;
const TAP_JUMP_UP: f32 = -0.7;

/// One player's raw controls for a single frame, already merged across that player's devices by the
/// impure layer. Movement is in [-1, 1] (`move_y` positive = down). Buttons split into `_held`
/// (level this frame) and `_pressed` (rising edge this frame); the impure layer owns edge detection
/// because that needs per-device history, the pure fold below only consumes the result.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct RawPad {
    pub move_x: f32,
    pub move_y: f32,
    pub c_x: f32, // c-stick (right stick): smash on the ground, aerial in the air
    pub c_y: f32,
    pub jump_held: bool,
    pub jump_pressed: bool,
    pub shorthop_pressed: bool,
    pub shield_held: bool,
    pub shield_pressed: bool,
    pub down_held: bool,
    pub down_pressed: bool,
    pub attack_held: bool,
    pub attack_pressed: bool,
    pub grab_pressed: bool,
    pub special_pressed: bool,
}

/// Per-player cross-frame memory the pure mapping carries (just the tap-jump flick edge for now).
/// One instance per player; the impure layer owns the storage and passes it back in each frame.
#[derive(Clone, Copy, Default, Debug)]
pub struct PadMemory {
    prev_move_y: f32,
}

impl PadMemory {
    /// Pure: fold one `RawPad` into a semantic `InputFrame`, advancing `self`. Tap-jump fires when
    /// `move_y` crosses from not-up into hard-up this frame (stick/keys flicked up), so it works the
    /// same for every device and needs no jump button.
    pub fn frame(&mut self, raw: &RawPad) -> InputFrame {
        let prev = self.prev_move_y;
        self.prev_move_y = raw.move_y;
        let tap_jump = prev > TAP_JUMP_PREV_UP && raw.move_y <= TAP_JUMP_UP;
        InputFrame {
            dir: raw.move_x,
            aim_y: raw.move_y,
            cx: raw.c_x,
            cy: raw.c_y,
            jump: tap_jump || raw.jump_pressed,
            jump_held: raw.jump_held,
            shorthop: raw.shorthop_pressed,
            shield_held: raw.shield_held,
            shield_pressed: raw.shield_pressed,
            down: raw.down_held,
            down_pressed: raw.down_pressed,
            attack: raw.attack_pressed,
            attack_held: raw.attack_held,
            grab: raw.grab_pressed,
            special: raw.special_pressed,
        }
    }
}

/// Synthesize a unit-deflection c-stick vector from four held direction keys (e.g. the keyboard's
/// arrow keys). A single direction gives a full +-1 deflection on that axis; an adjacent pair (a
/// diagonal) normalizes to unit magnitude, the same convention the analog stick's deadzone/normalize
/// path uses. Opposite keys held together cancel; nothing held is neutral. Either way, all-neutral
/// is (0.0, 0.0) so the impure caller can test-and-skip instead of stomping a pad/touch c-stick read.
pub fn dpad_to_cstick(up: bool, down: bool, left: bool, right: bool) -> (f32, f32) {
    let x = (right as i32 - left as i32) as f32;
    let y = (down as i32 - up as i32) as f32;
    let mag = (x * x + y * y).sqrt();
    if mag == 0.0 {
        (0.0, 0.0)
    } else {
        (x / mag, y / mag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpad_to_cstick_neutral_is_zero() {
        assert_eq!(dpad_to_cstick(false, false, false, false), (0.0, 0.0));
    }

    #[test]
    fn dpad_to_cstick_opposite_keys_cancel() {
        assert_eq!(dpad_to_cstick(true, true, false, false), (0.0, 0.0)); // up+down
        assert_eq!(dpad_to_cstick(false, false, true, true), (0.0, 0.0)); // left+right
    }

    #[test]
    fn dpad_to_cstick_cardinal_is_unit_deflection() {
        let (x, y) = dpad_to_cstick(false, false, false, true); // right
        assert!((x - 1.0).abs() < 1e-6 && y == 0.0);
        let (x, y) = dpad_to_cstick(true, false, false, false); // up
        assert!(x == 0.0 && (y - (-1.0)).abs() < 1e-6);
        let (x, y) = dpad_to_cstick(false, true, false, false); // down
        assert!(x == 0.0 && (y - 1.0).abs() < 1e-6);
        let (x, y) = dpad_to_cstick(false, false, true, false); // left
        assert!((x - (-1.0)).abs() < 1e-6 && y == 0.0);
    }

    #[test]
    fn dpad_to_cstick_diagonal_normalizes_to_unit_magnitude() {
        let (x, y) = dpad_to_cstick(true, false, false, true); // up-right
        let mag = (x * x + y * y).sqrt();
        assert!(
            (mag - 1.0).abs() < 1e-6,
            "diagonal must be unit magnitude, got {mag}"
        );
        assert!(x > 0.0 && y < 0.0);
    }

    #[test]
    fn tap_jump_fires_on_up_flick_then_not_on_hold() {
        let mut mem = PadMemory::default();
        assert!(!mem.frame(&RawPad::default()).jump, "neutral never jumps");
        let up = RawPad {
            move_y: -1.0,
            ..Default::default()
        };
        assert!(mem.frame(&up).jump, "flick into hard-up taps jump");
        assert!(!mem.frame(&up).jump, "held up has no fresh edge, no jump");
    }

    #[test]
    fn button_jump_passes_through() {
        let mut mem = PadMemory::default();
        let raw = RawPad {
            jump_pressed: true,
            ..Default::default()
        };
        assert!(mem.frame(&raw).jump);
    }

    #[test]
    fn tap_jump_fires_on_top_left_flick() {
        let mut mem = PadMemory::default();
        let top_left = RawPad {
            move_x: -0.9,
            move_y: -0.8,
            ..Default::default()
        };
        assert!(
            mem.frame(&top_left).jump,
            "diagonal top-left over hard up edge taps jump"
        );
        assert!(
            !mem.frame(&top_left).jump,
            "holding top-left does not keep jumping"
        );
    }

    #[test]
    fn held_and_pressed_map_independently() {
        let mut mem = PadMemory::default();
        let raw = RawPad {
            attack_held: true,
            attack_pressed: false,
            shield_held: true,
            shield_pressed: true,
            ..Default::default()
        };
        let f = mem.frame(&raw);
        assert!(f.attack_held && !f.attack, "held without a fresh press");
        assert!(f.shield_held && f.shield_pressed);
    }
}
