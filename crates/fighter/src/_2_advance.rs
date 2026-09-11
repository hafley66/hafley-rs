//! One fixed tick of locomotion, shaped after the Melee decomp ftCommon
//! motion states. Unsupported in this cut: attacks (ATTACK bit is read but
//! has no effect), ledges, walls, platforms.

use crate::_0_rules::Rules;
use crate::_1_state::{Input, Phase, State, button};
use crate::_1a_chart::{self, Event};
use crate::_1b_ground::Facts;
use crate::_1c_air::AirFacts;

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
    clamp_horizontal(vel, cap);
}

fn clamp_horizontal(vel: &mut f32, cap: f32) {
    let cap = cap.abs();
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
            self.velocity[0] += init;
        } else {
            self.velocity[0] = init;
        }
        clamp_horizontal(&mut self.velocity[0], r.ground_max_horizontal_velocity);
        self.enter(Phase::Dash);
    }

    fn start_walk(&mut self, axis: f32, r: &Rules) {
        self.facing = axis.signum();
        self.velocity[0] = axis.signum() * r.walk_init_vel;
        self.enter(Phase::Walk);
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
        let facts = Facts {
            down: input.buttons & button::DOWN != 0,
            dash: input.axis.abs() >= r.dash_stick_threshold,
            walk: can_walk && input.axis.abs() >= r.walk_stick_threshold,
            ..Facts::default()
        };
        match _1a_chart::decide(self.phase, Event::GroundIntent(facts)) {
            Some(Phase::Dash) => self.start_dash(input.axis, r),
            Some(Phase::Walk) => self.start_walk(input.axis, r),
            Some(next) => self.enter(next),
            None => {}
        }
    }

    /// One fixed tick. Caller owns tick cadence; Melee cadence is 60 Hz.
    #[tracing::instrument(target = "game_fighter::tick", level = "trace", skip_all)]
    pub(crate) fn advance_impl(&mut self, input: Input, r: &Rules) {
        if !input.axis.is_finite() {
            return;
        }
        let frame = self.input_history.advance(game_input::PlayerInput {
            buttons: u32::from(input.buttons),
            axes: [game_input::quantize_axis(input.axis), 0, 0, 0],
        });

        // Hitlag freezes position/velocity integration and action clocks for its
        // exact stored frame count.
        if self.combat.hitlag > 0 {
            self.combat.hitlag -= 1;
            return;
        }
        // Hitstun suppresses ordinary movement and jump control while launch
        // motion advances deterministically.
        if self.combat.hitstun > 0 {
            self.combat.hitstun -= 1;
            self.hitstun_step(r);
            return;
        }

        let jump_released = frame.released as u8 & button::JUMP != 0;
        let jump_pressed = frame.pressed as u8 & button::JUMP != 0;
        let down_held = input.buttons & button::DOWN != 0;
        let down_pressed = down_held && frame.previous.buttons as u8 & button::DOWN == 0;

        let jump_starts_squat =
            jump_pressed && _1a_chart::decide(self.phase, Event::JumpRequest) == Some(Phase::Squat);
        let facts = Facts {
            dash: input.axis.abs() >= r.dash_stick_threshold,
            walk: input.axis.abs() >= r.walk_stick_threshold,
            forward: input.axis * self.facing >= r.dash_stick_threshold,
            reverse: input.axis * self.facing < 0.0 && input.axis.abs() >= r.dash_stick_threshold,
            down: down_held,
            ..Facts::default()
        };

        if jump_starts_squat {
            self.short_hop = false;
            self.enter(Phase::Squat);
        } else {
            match self.phase {
                Phase::Idle => {
                    apply_ground_friction(&mut self.velocity[0], r.ground_friction);
                    self.ground_common(input, r, true);
                }
                Phase::Walk => {
                    if jump_pressed {
                        unreachable!("ground jump edge is handled before walk physics");
                    }
                    // ftwalkcommon.c:190-198: tapered accel toward stick target.
                    let target = input.axis * r.walk_max_vel;
                    if target != 0.0 && self.velocity[0] * target >= 0.0 {
                        let accel =
                            input.axis.signum() * r.walk_accel * (1.0 - self.velocity[0] / target);
                        approach(
                            &mut self.velocity[0],
                            accel,
                            target,
                            r.ground_max_horizontal_velocity,
                        );
                    } else {
                        apply_ground_friction(&mut self.velocity[0], r.ground_friction);
                    }
                    match _1a_chart::decide(self.phase, Event::Motion(facts)) {
                        Some(Phase::Dash) => self.start_dash(input.axis, r),
                        Some(next) => self.enter(next),
                        None => self.facing = input.axis.signum(),
                    }
                }
                Phase::Dash => {
                    match _1a_chart::decide(
                        self.phase,
                        Event::Motion(Facts {
                            finished: self.phase_tick >= r.dash_ticks,
                            ..facts
                        }),
                    ) {
                        Some(Phase::Dash) => self.start_dash(input.axis, r),
                        Some(Phase::Run) => {
                            self.enter(Phase::Run);
                            dash_run_step(&mut self.velocity[0], input.axis, r);
                        }
                        Some(next) => self.enter(next),
                        None => dash_run_step(&mut self.velocity[0], input.axis, r),
                    }
                    if self.phase == Phase::Dash && down_held {
                        self.ground_common(input, r, false);
                    }
                }
                Phase::Run => {
                    dash_run_step(&mut self.velocity[0], input.axis, r);
                    if let Some(next) = _1a_chart::decide(self.phase, Event::Motion(facts)) {
                        self.enter(next);
                    }
                }
                Phase::Brake => {
                    apply_ground_friction(&mut self.velocity[0], r.ground_friction);
                    if let Some(next) = _1a_chart::decide(
                        self.phase,
                        Event::Motion(Facts {
                            stopped: self.velocity[0] == 0.0,
                            ..facts
                        }),
                    ) {
                        self.enter(next);
                        self.ground_common(input, r, true);
                    }
                }
                Phase::Turn => {
                    // ftCo_Turn.c:69-86: countdown, flip, pivot window.
                    apply_ground_friction(&mut self.velocity[0], r.ground_friction);
                    let wanted = -self.facing;
                    match _1a_chart::decide(
                        self.phase,
                        Event::Motion(Facts {
                            finished: self.phase_tick >= r.turn_ticks,
                            ..facts
                        }),
                    ) {
                        Some(Phase::Dash) => self.start_dash(input.axis, r),
                        Some(next) => {
                            self.facing = wanted;
                            self.enter(next);
                            self.ground_common(input, r, true);
                        }
                        None => {}
                    }
                }
                Phase::Squat => {
                    apply_ground_friction(&mut self.velocity[0], r.ground_friction);
                    if jump_released {
                        self.short_hop = true;
                    }
                    if _1a_chart::decide(
                        self.phase,
                        Event::Motion(Facts {
                            finished: self.phase_tick + 1 >= r.jump_startup_time,
                            ..facts
                        }),
                    ) == Some(Phase::Jump)
                    {
                        self.takeoff(input.axis, r);
                    }
                }
                Phase::CrouchEnter => {
                    apply_ground_friction(&mut self.velocity[0], r.ground_friction);
                    if let Some(next) = _1a_chart::decide(
                        self.phase,
                        Event::Motion(Facts {
                            finished: self.phase_tick >= r.crouch_enter_ticks,
                            ..facts
                        }),
                    ) {
                        self.enter(next);
                    }
                }
                Phase::CrouchHold => {
                    apply_ground_friction(&mut self.velocity[0], r.ground_friction);
                    if let Some(next) = _1a_chart::decide(self.phase, Event::Motion(facts)) {
                        self.enter(next);
                        self.ground_common(input, r, true);
                    }
                }
                Phase::CrouchExit => {
                    apply_ground_friction(&mut self.velocity[0], r.ground_friction);
                    if let Some(next) = _1a_chart::decide(
                        self.phase,
                        Event::Motion(Facts {
                            finished: self.phase_tick >= r.crouch_exit_ticks,
                            ..facts
                        }),
                    ) {
                        self.enter(next);
                        self.ground_common(input, r, true);
                    }
                }
                Phase::Landing => {
                    apply_ground_friction(&mut self.velocity[0], r.ground_friction);
                    if let Some(next) = _1a_chart::decide(
                        self.phase,
                        Event::Motion(Facts {
                            finished: self.phase_tick + 1 >= r.landing_lag,
                            ..facts
                        }),
                    ) {
                        self.enter(next);
                    }
                }
                Phase::Jump | Phase::AirJump => {
                    self.air_step(input, r, jump_pressed, down_pressed);
                }
                Phase::Fall => {
                    self.air_step(input, r, jump_pressed, down_pressed);
                }
            }
        }

        // integrate
        let airborne = !self.phase.grounded();
        let takeoff_tick = airborne
            && (self.phase == Phase::Jump || self.phase == Phase::AirJump)
            && self.phase_tick == 0;
        if airborne {
            // Melee accumulates the launch velocity before applying gravity.
            self.position[0] += self.velocity[0];
            self.position[1] += self.velocity[1];
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
            let facts = AirFacts {
                descending: self.velocity[1] <= 0.0,
                jump_pressed: false,
                jumps_left: self.jumps_left,
            };
            if let Some(next) = _1a_chart::decide(self.phase, Event::AirMotion(facts)) {
                self.enter_air(next, input, r);
            }
        } else {
            self.position[0] += self.velocity[0];
            self.position[1] += self.velocity[1];
        }

        if airborne && self.position[1] <= 0.0 && self.velocity[1] < 0.0 {
            if let Some(next) = _1a_chart::decide(self.phase, Event::Land) {
                self.enter_air(next, input, r);
            }
        }

        self.phase_tick += 1;
    }

    /// Launch motion under hitstun: no movement or jump control, deterministic
    /// airborne integration. Ground contact ends hitstun and enters `Landing`.
    /// Action clocks stay frozen; landing tech and knockback decay are not
    /// modeled in this cut.
    fn hitstun_step(&mut self, r: &Rules) {
        self.position[0] += self.velocity[0];
        self.position[1] += self.velocity[1];
        self.velocity[1] -= r.gravity;
        let floor = if self.fast_fall {
            -r.fast_fall_velocity
        } else {
            -r.terminal_velocity
        };
        if self.velocity[1] < floor {
            self.velocity[1] = floor;
        }
        if self.position[1] <= 0.0 && self.velocity[1] < 0.0 {
            self.position[1] = 0.0;
            self.velocity = [0.0, 0.0];
            self.jumps_left = r.max_jumps;
            self.fast_fall = false;
            self.combat.hitstun = 0;
            self.combat.tumble = false;
            self.enter(Phase::Landing);
        }
    }

    /// Apply the numeric entry effect for an air chart destination exactly once,
    /// then enter the phase. The chart owns permission and destination only.
    fn enter_air(&mut self, next: Phase, input: Input, r: &Rules) {
        match next {
            Phase::AirJump => {
                self.velocity[0] = input.axis * r.air_jump_h_multiplier;
                self.velocity[1] = r.jump_v_initial_velocity * r.air_jump_v_multiplier;
                self.jumps_left -= 1;
                self.fast_fall = false;
            }
            Phase::Landing => {
                self.position[1] = 0.0;
                self.velocity[1] = 0.0;
                self.jumps_left = r.max_jumps;
                self.fast_fall = false;
            }
            _ => {}
        }
        self.enter(next);
    }

    /// Air drift + fast fall + double jump (ftCo_Fall.c:164-183,
    /// ftCo_JumpAerial.c:99-101,175-177).
    fn air_step(&mut self, input: Input, r: &Rules, jump_pressed: bool, down_pressed: bool) {
        if down_pressed && self.velocity[1] < 0.0 {
            self.fast_fall = true;
        }
        let facts = AirFacts {
            // The jump callback runs before gravity; descending is decided post-integration.
            descending: false,
            jump_pressed,
            jumps_left: self.jumps_left,
        };
        if let Some(next) = _1a_chart::decide(self.phase, Event::AirMotion(facts)) {
            self.enter_air(next, input, r);
            return;
        }
        let target = input.axis * r.air_drift_max;
        if target != 0.0 {
            // Attribute pair stays separate at partial stick deflection.
            // Excess launch momentum uses this slice's friction policy;
            // complete source callback ordering remains unqualified.
            let excess_launch_velocity = self.velocity[0].abs() > r.air_drift_max;
            let accel = if excess_launch_velocity && self.velocity[0] * input.axis > 0.0 {
                -self.velocity[0].signum() * r.aerial_friction
            } else {
                input.axis * r.air_drift_stick_mul + input.axis.signum() * r.air_drift_base
            };
            self.velocity[0] += accel;
            if !excess_launch_velocity && (self.velocity[0] - target) * input.axis > 0.0 {
                self.velocity[0] = target;
            }
        } else if input.axis == 0.0 || self.velocity[0] * input.axis < 0.0 {
            apply_air_friction(&mut self.velocity[0], r.aerial_friction);
        }
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
