//! One frame of sampled device input. Shared shell type: the raw-device samplers in `controls/`
//! produce it, and both the V1 sim and the V4 native-control router consume it. Lives outside
//! `v1` so the V4 path never imports through the legacy module.

/// One frame of input, sampled at the edge and fed to a pure step.
#[derive(Copy, Clone, Default)]
pub struct InputFrame {
    pub dir: f32,             // stick x, -1..1
    pub aim_y: f32,           // stick y, -1 up .. +1 down (air-dodge / wavedash aim)
    pub cx: f32,              // c-stick x, -1..1 (smash on the ground, aerial in the air)
    pub cy: f32,              // c-stick y, -1 up .. +1 down (same convention as aim_y)
    pub jump: bool,           // jump pressed THIS frame (rising edge) -> full hop
    pub jump_held: bool,      // jump currently held (release before takeoff = short hop)
    pub shorthop: bool,       // dedicated short-hop pressed THIS frame
    pub shield_held: bool,    // shield button held (grounded -> Shield)
    pub shield_pressed: bool, // shield pressed THIS frame (airborne -> AirDodge)
    pub down: bool,           // down held (fast fall / spot dodge / soft-platform drop)
    pub down_pressed: bool,   // down pressed THIS frame (deliberate ledge drop)
    pub attack: bool,         // attack pressed THIS frame (jab / aerial / pickup / fire)
    pub attack_held: bool,    // attack currently held (full-auto gun fire)
    pub grab: bool,           // grab pressed THIS frame (drop a held item)
    pub special: bool,        // special (B) pressed THIS frame; stick at press picks N/Side/Up/Down
}
