//! One fixed tick of locomotion, shaped after the Melee decomp ftCommon
//! motion states. Unsupported in this cut: attacks (ATTACK bit is read but
//! has no effect), ledges, walls, platforms, crouch variants beyond hold.

use crate::rules::Rules;
use crate::state::{Input, Phase, State, button};

/// Linear ground friction toward zero, clamped to not overshoot
/// (`ftCommon_ApplyFrictionGround`, ft/ftcommon.c:50-60).
fn apply_ground_friction(vel: &mut f32, friction: f32) {
    let magnitude = friction.abs();
    if magnitude >= vel.abs() {
        *vel = 0.0;
    } else {
        *vel -= vel.signum() * magnitude;
    }
}

/// Accelerate `vel` toward `target` by `accel` without overshooting the
/// target or the ground velocity cap (`ftCommon_8007C98C`,
/// ft/ftcommon.c:67-105).
fn approach(vel: &mut f32, accel: f32, target: f32, cap: f32) {
    if target == 0.0 {
        return;
    }
    if *vel * accel < 0.0 {
        return;
    }
    let candidate = *vel + accel;
    let overshot = if accel > 0.0 {
        candidate > target
    } else {
        candidate < target
    };
    if overshot {
        *vel = target;
    } else {
        *vel = candidate;
    }
    if vel.abs() > cap {
        *vel = vel.signum() * cap;
    }
}

/// Dash/run acceleration pair (`getAccelAndTarget`, ft/inlines.h:135-145):
/// accel = stick*dash_accel_mul + sign(stick)*dash_accel_base,
/// target = stick*dash_max_velocity.
fn dash_run_step(vel: &mut f32, axis: f32, r: &Rules) {
    let accel = axis * r.dash_accel_mul + axis.signum() * r.dash_accel_base;
    let target = axis * r.dash_max_velocity;
    approach(vel, accel, target, r.ground_max_horizontal_velocity);
}

impl State {
    fn start_dash(&mut self, axis: f32, r: &Rules) {
        self.facing = axis.signum();
        // ft/kinds/ftCommon/ftCo_Dash.c:63-69
        let init = self.facing * r.dash_initial_velocity;
        if self.velocity[0] * self.facing < 0.0 {
            self.velocity[0] = init - self.velocity[0];
        } else {
            self.velocity[0] = init;
        }
        self.enter(Phase::Dash);
    }

    fn takeoff(&mut self, axis: f32, r: &Rules) {
        // ftCo_Jump.c:115-145
        self.velocity[0] *= r.ground_to_air_jump_momentum_multiplier;
        self.velocity[1] = if self.short_hop {
            r.hop_v_initial_velocity
        } else {
            r.jump_v_initial_velocity
        };
        let mut h = self.velocity[0] + axis * r.jump_h_initial_velocity;
        if h.abs() > r.jump_h_max_velocity {
            h = h.signum() * r.jump_h_max_velocity;
        }
        self.velocity[0] = h;
        self.jumps_left -= 1;
        self.fast_fall = false;
        self.enter(Phase::Jump);
    }

    fn ground_common(&mut self, input: Input, r: &Rules, can_walk: bool) {
        let jump_pressed =
            input.buttons & button::JUMP != 0 && self.previous_buttons & button::JUMP == 0;
        if jump_pressed {
            self.short_hop = false;
            self.enter(Phase::Squat);
            return;
        }
        if input.buttons & button::DOWN != 0 {
            self.enter(Phase::Crouch);
            return;
        }
        if input.axis.abs() >= r.dash_stick_threshold {
            self.start_dash(input.axis, r);
        } else if can_walk && input.axis.abs() >= r.walk_stick_threshold {
            self.enter(Phase::Walk);
        }
    }

    /// One fixed tick. Caller owns tick cadence; Melee cadence is 60 Hz.
    #[tracing::instrument(target = "game_fighter::tick", level = "trace", skip_all)]
    pub fn advance(&mut self, input: Input, r: &Rules) {
        let jump_released =
            self.previous_buttons & button::JUMP != 0 && input.buttons & button::JUMP == 0;
        let jump_pressed =
            input.buttons & button::JUMP != 0 && self.previous_buttons & button::JUMP == 0;
        let down_held = input.buttons & button::DOWN != 0;

        match self.phase {
            Phase::Idle => {
                apply_ground_friction(&mut self.velocity[0], r.ground_friction);
                self.ground_common(input, r, true);
            }
            Phase::Walk => {
                // ftwalkcommon.c:190-198: tapered accel toward stick target.
                let target = input.axis * r.walk_max_vel;
                if target != 0.0 && self.velocity[0] * target >= 0.0 {
                    let accel = r.walk_accel * (1.0 - self.velocity[0] / target);
                    approach(
                        &mut self.velocity[0],
                        accel,
                        target,
                        r.ground_max_horizontal_velocity,
                    );
                } else {
                    apply_ground_friction(&mut self.velocity[0], r.ground_friction);
                }
                if self.phase == Phase::Walk {
                    if input.axis.abs() >= r.dash_stick_threshold {
                        self.start_dash(input.axis, r);
                    } else if input.axis.abs() < r.walk_stick_threshold {
                        self.enter(Phase::Idle);
                    } else {
                        self.facing = input.axis.signum();
                    }
                }
            }
            Phase::Dash => {
                if input.axis.abs() >= r.dash_stick_threshold
                    && input.axis.signum() != self.facing
                {
                    // dash-dance reversal
                    self.start_dash(input.axis, r);
                } else if input.axis * self.facing >= r.dash_stick_threshold {
                    if self.phase_tick >= r.dash_ticks {
                        self.enter(Phase::Run);
                        dash_run_step(&mut self.velocity[0], input.axis, r);
                    } else {
                        // multiplicative dash-sustain decay
                        // (ftCo_Dash.c:142-144) then accel
                        self.velocity[0] -=
                            self.velocity[0] * r.dash_friction_mul * r.ground_friction;
                        dash_run_step(&mut self.velocity[0], input.axis, r);
                    }
                } else {
                    self.enter(Phase::Brake);
                }
                if self.phase == Phase::Dash {
                    if jump_pressed || down_held {
                        self.ground_common(input, r, false);
                    }
                }
            }
            Phase::Run => {
                dash_run_step(&mut self.velocity[0], input.axis, r);
                if input.axis * self.facing < 0.0 && input.axis.abs() >= r.dash_stick_threshold {
                    self.enter(Phase::Turn);
                } else if input.axis.abs() < r.walk_stick_threshold {
                    self.enter(Phase::Brake);
                } else if jump_pressed || down_held {
                    self.ground_common(input, r, false);
                }
            }
            Phase::Brake => {
                apply_ground_friction(&mut self.velocity[0], r.ground_friction);
                if self.velocity[0] == 0.0 {
                    self.enter(Phase::Idle);
                    self.ground_common(input, r, true);
                }
            }
            Phase::Turn => {
                // ftCo_Turn.c:69-86: countdown, flip, pivot window.
                apply_ground_friction(&mut self.velocity[0], r.ground_friction);
                let wanted = -self.facing;
                if input.axis * wanted >= r.dash_stick_threshold {
                    self.start_dash(input.axis, r);
                } else if self.phase_tick >= r.turn_ticks {
                    self.facing = wanted;
                    self.enter(Phase::Idle);
                    self.ground_common(input, r, true);
                }
            }
            Phase::Squat => {
                apply_ground_friction(&mut self.velocity[0], r.ground_friction);
                if jump_released {
                    self.short_hop = true;
                }
                if self.phase_tick + 1 >= r.jump_startup_time {
                    self.takeoff(input.axis, r);
                }
            }
            Phase::Crouch => {
                apply_ground_friction(&mut self.velocity[0], r.ground_friction);
                if !down_held {
                    self.enter(Phase::Idle);
                    self.ground_common(input, r, true);
                }
            }
            Phase::Landing => {
                apply_ground_friction(&mut self.velocity[0], r.ground_friction);
                if self.phase_tick + 1 >= r.landing_lag {
                    self.enter(Phase::Idle);
                }
            }
            Phase::Jump | Phase::AirJump => {
                self.enter(Phase::Fall);
                self.air_step(input, r, jump_pressed);
            }
            Phase::Fall => {
                self.air_step(input, r, jump_pressed);
            }
        }

        // integrate
        let grounded = self.phase.grounded();
        if !grounded {
            // gravity, then clamp (ssbm_utils calc::general::jump_arc order:
            // position accumulates before velocity decays; frame 1 after
            // takeoff skips gravity)
            let takeoff_tick =
                (self.phase == Phase::Jump || self.phase == Phase::AirJump) && self.phase_tick == 0;
            if !takeoff_tick {
                self.velocity[1] -= r.gravity;
            }
            let floor = if self.fast_fall {
                -r.fast_fall_velocity
            } else {
                -r.terminal_velocity
            };
            if self.velocity[1] < floor {
                self.velocity[1] = floor;
            }
        }
        self.position[0] += self.velocity[0];
        self.position[1] += self.velocity[1];

        if !grounded && self.position[1] <= 0.0 && self.velocity[1] < 0.0 {
            self.position[1] = 0.0;
            self.velocity[1] = 0.0;
            self.jumps_left = r.max_jumps;
            self.fast_fall = false;
            self.enter(Phase::Landing);
        }

        self.previous_buttons = input.buttons;
        self.previous_axis = input.axis;
        self.phase_tick += 1;
    }

    /// Air drift + fast fall + double jump (ftCo_Fall.c:164-183,
    /// ftCo_JumpAerial.c:99-101,175-177).
    fn air_step(&mut self, input: Input, r: &Rules, jump_pressed: bool) {
        if input.buttons & button::DOWN != 0
            && self.previous_buttons & button::DOWN == 0
            && self.velocity[1] < 0.0
        {
            self.fast_fall = true;
        }
        if jump_pressed && self.jumps_left > 0 {
            self.velocity[0] = input.axis * r.air_jump_h_multiplier;
            self.velocity[1] = r.jump_v_initial_velocity * r.air_jump_v_multiplier;
            self.jumps_left -= 1;
            self.fast_fall = false;
            self.enter(Phase::AirJump);
            return;
        }
        let target = input.axis * r.air_drift_max;
        if target != 0.0 && self.velocity[0] * target >= 0.0 {
            // tapered drift: accel shrinks as vel/air_drift_max -> 1
            let frac = (self.velocity[0] / r.air_drift_max).clamp(-1.0, 1.0);
            let accel = input.axis * r.air_drift_stick_mul * (1.0 - frac * input.axis.signum());
            if self.velocity[0] * accel >= 0.0 {
                self.velocity[0] += accel;
                if (self.velocity[0] - target) * input.axis > 0.0 {
                    self.velocity[0] = target;
                }
            }
        } else if input.axis == 0.0 || self.velocity[0] * input.axis < 0.0 {
            apply_air_friction(&mut self.velocity[0], r.aerial_friction);
        }
        self.facing = if input.axis != 0.0 {
            input.axis.signum()
        } else {
            self.facing
        };
    }
}

/// Constant-magnitude air friction toward zero (aerial_friction).
fn apply_air_friction(vel: &mut f32, friction: f32) {
    apply_ground_friction(vel, friction);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friction_is_linear_not_multiplicative() {
        let mut v = 1.0_f32;
        apply_ground_friction(&mut v, 0.1);
        assert_eq!(v, 0.9);
        apply_ground_friction(&mut v, 0.95);
        assert_eq!(v, 0.0);
    }

    #[test]
    fn approach_clamps_to_target() {
        let mut v = 0.0_f32;
        approach(&mut v, 0.5, 0.85, 10.0);
        assert_eq!(v, 0.5);
        approach(&mut v, 0.5, 0.85, 10.0);
        assert_eq!(v, 0.85);
    }
}
