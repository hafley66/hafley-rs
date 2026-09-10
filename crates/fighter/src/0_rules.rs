//! Caller-supplied numeric parameters for one fighter's locomotion.
//!
//! Field names follow the decomp attribute vocabulary (`ftCo_DatAttrs` /
//! `getAccelAndTarget`, ft/inlines.h:135-145). No defaults: every fighter
//! supplies the full set. The decomp's `dash_accel_base`/`dash_accel_mul`
//! appear as `dash_run_acceleration_a`/`_b` in some decomp revisions.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rules {
    // walk
    pub walk_init_vel: f32,
    pub walk_accel: f32,
    pub walk_max_vel: f32,
    pub walk_stick_threshold: f32,
    // dash / run
    pub dash_stick_threshold: f32,
    pub dash_initial_velocity: f32,
    pub dash_accel_base: f32,
    pub dash_accel_mul: f32,
    pub dash_max_velocity: f32,
    pub dash_ticks: u32,
    // ground
    pub ground_friction: f32,
    pub dash_friction_mul: f32,
    pub ground_max_horizontal_velocity: f32,
    pub turn_ticks: u32,
    // jumpsquat / takeoff
    pub jump_startup_time: u32,
    // crouch lifecycle (source Squat/SquatRv animation completion)
    pub crouch_enter_ticks: u32,
    pub crouch_exit_ticks: u32,
    pub jump_h_initial_velocity: f32,
    pub jump_h_max_velocity: f32,
    pub jump_v_initial_velocity: f32,
    pub hop_v_initial_velocity: f32,
    pub ground_to_air_jump_momentum_multiplier: f32,
    pub max_jumps: u8,
    pub air_jump_v_multiplier: f32,
    pub air_jump_h_multiplier: f32,
    // air
    pub gravity: f32,
    pub terminal_velocity: f32,
    pub fast_fall_velocity: f32,
    pub air_drift_stick_mul: f32,
    pub air_drift_base: f32,
    pub air_drift_max: f32,
    pub aerial_friction: f32,
    // landing
    pub landing_lag: u32,
}
