//! Authored evaluator for the generated neutral rule data.
//!
//! The generated module owns rule facts (predicates, ordered effect templates
//! and source provenance); this module owns their interpretation. Evaluation is
//! stack-only: no heap, no game memory, no dependency.

use crate::{
    AttrField, CommonTuning, CompareOp, Effect, Expr, FighterAttrs, FighterInput, InputField,
    LocalSlot, MechanicEffect, Motion, Rule, RuleId, Statement, TuningField, Vec3, RULES,
};

/// Local values bound earlier in a rule body.
#[derive(Clone, Copy, Default)]
struct Locals {
    motion: Option<Motion>,
    velocity: Option<Vec3>,
}

impl Locals {
    fn set(&mut self, slot: LocalSlot, value: Value) {
        match slot {
            LocalSlot::Motion => self.motion = Some(value.motion()),
            LocalSlot::Velocity => self.velocity = Some(value.vector()),
        }
    }
}

/// A typed value produced while evaluating a rule expression.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Value {
    Number(f32),
    Count(u32),
    Flag(bool),
    Motion(Motion),
    Vector(Vec3),
}

impl Value {
    fn number(self) -> f32 {
        match self {
            Value::Number(value) => value,
            other => panic!("expected a number, got {other:?}"),
        }
    }

    fn count(self) -> u32 {
        match self {
            Value::Count(value) => value,
            other => panic!("expected a count, got {other:?}"),
        }
    }

    fn flag(self) -> bool {
        match self {
            Value::Flag(value) => value,
            other => panic!("expected a flag, got {other:?}"),
        }
    }

    fn motion(self) -> Motion {
        match self {
            Value::Motion(value) => value,
            other => panic!("expected a motion, got {other:?}"),
        }
    }

    fn vector(self) -> Vec3 {
        match self {
            Value::Vector(value) => value,
            other => panic!("expected a vector, got {other:?}"),
        }
    }
}

fn compare(op: CompareOp, lhs: f32, rhs: f32) -> bool {
    match op {
        CompareOp::Less => lhs < rhs,
        CompareOp::LessEqual => lhs <= rhs,
        CompareOp::Greater => lhs > rhs,
        CompareOp::GreaterEqual => lhs >= rhs,
        CompareOp::Equal => lhs == rhs,
        CompareOp::NotEqual => lhs != rhs,
    }
}

fn eval(
    expr: &Expr,
    input: &FighterInput,
    tuning: &CommonTuning,
    attrs: Option<&FighterAttrs>,
    locals: &Locals,
) -> Value {
    match expr {
        Expr::Input(InputField::StickX) => Value::Number(input.stick_x),
        Expr::Input(InputField::Facing) => Value::Number(input.facing),
        Expr::Tuning(TuningField::TurnThreshold) => Value::Number(tuning.turn_threshold),
        Expr::Tuning(TuningField::JumpBackThreshold) => {
            Value::Number(tuning.jump_back_threshold)
        }
        Expr::Attr(AttrField::AirJumpHScale) => {
            Value::Number(attrs.expect("rule needs attributes").air_jump_h_scale)
        }
        Expr::Attr(AttrField::JumpInitialSpeed) => {
            Value::Number(attrs.expect("rule needs attributes").jump_initial_speed)
        }
        Expr::Attr(AttrField::AirJumpVScale) => {
            Value::Number(attrs.expect("rule needs attributes").air_jump_v_scale)
        }
        Expr::Local(LocalSlot::Motion) => Value::Motion(locals.motion.expect("motion bound")),
        Expr::Local(LocalSlot::Velocity) => {
            Value::Vector(locals.velocity.expect("velocity bound"))
        }
        Expr::Motion(motion) => Value::Motion(*motion),
        Expr::Flag(value) => Value::Flag(*value),
        Expr::Number(value) => Value::Number(*value),
        Expr::Count(value) => Value::Count(*value),
        Expr::Neg(inner) => Value::Number(-eval(inner, input, tuning, attrs, locals).number()),
        Expr::Mul(lhs, rhs) => {
            let lhs = eval(lhs, input, tuning, attrs, locals).number();
            let rhs = eval(rhs, input, tuning, attrs, locals).number();
            Value::Number(lhs * rhs)
        }
        Expr::Compare(lhs, op, rhs) => {
            let lhs = eval(lhs, input, tuning, attrs, locals).number();
            let rhs = eval(rhs, input, tuning, attrs, locals).number();
            Value::Flag(compare(*op, lhs, rhs))
        }
        Expr::Select {
            condition,
            yes,
            no,
        } => {
            if eval(condition, input, tuning, attrs, locals).flag() {
                eval(yes, input, tuning, attrs, locals)
            } else {
                eval(no, input, tuning, attrs, locals)
            }
        }
        Expr::Vector { x, y, z } => Value::Vector(Vec3 {
            x: eval(x, input, tuning, attrs, locals).number(),
            y: eval(y, input, tuning, attrs, locals).number(),
            z: eval(z, input, tuning, attrs, locals).number(),
        }),
    }
}

fn build_effect(
    effect: &Effect,
    input: &FighterInput,
    tuning: &CommonTuning,
    attrs: Option<&FighterAttrs>,
    locals: &Locals,
) -> MechanicEffect {
    match effect {
        Effect::BeginJump => MechanicEffect::BeginJump,
        Effect::EnterMotion {
            motion,
            flags,
            anim_start,
            anim_speed,
            anim_blend,
        } => MechanicEffect::EnterMotion {
            motion: eval(motion, input, tuning, attrs, locals).motion(),
            flags: *flags,
            anim_start: *anim_start,
            anim_speed: *anim_speed,
            anim_blend: *anim_blend,
        },
        Effect::SetJumpParams { enabled, scale } => MechanicEffect::SetJumpParams {
            enabled: eval(enabled, input, tuning, attrs, locals).flag(),
            scale: *scale,
        },
        Effect::SetCommandValue { slot, value } => MechanicEffect::SetCommandValue {
            slot: *slot,
            value: eval(value, input, tuning, attrs, locals).count(),
        },
        Effect::SetFlag { value } => MechanicEffect::SetFlag {
            value: eval(value, input, tuning, attrs, locals).flag(),
        },
        Effect::LaunchJump {
            motion,
            velocity,
            flag,
        } => MechanicEffect::LaunchJump {
            motion: eval(motion, input, tuning, attrs, locals).motion(),
            velocity: match velocity {
                LocalSlot::Velocity => locals.velocity.expect("velocity bound"),
                LocalSlot::Motion => panic!("launch velocity must be the velocity local"),
            },
            flag: eval(flag, input, tuning, attrs, locals).flag(),
        },
    }
}

fn rule(id: RuleId) -> &'static Rule {
    RULES
        .iter()
        .find(|rule| rule.id == id)
        .expect("generated rule present")
}

/// Evaluate the neutral `turn_request` predicate.
pub fn turn_request(input: &FighterInput, tuning: &CommonTuning) -> bool {
    let rule = rule(RuleId::TurnRequest);
    let guard = rule.guard.expect("turn_request is a predicate rule");
    eval(guard, input, tuning, None, &Locals::default()).flag()
}

fn run<const N: usize>(
    id: RuleId,
    input: &FighterInput,
    tuning: &CommonTuning,
    attrs: Option<&FighterAttrs>,
) -> [MechanicEffect; N] {
    let rule = rule(id);
    let mut locals = Locals::default();
    let mut out: [Option<MechanicEffect>; N] = [None; N];
    let mut emitted = 0;
    for statement in rule.statements {
        match statement {
            Statement::Bind { slot, value } => {
                let value = eval(value, input, tuning, attrs, &locals);
                locals.set(*slot, value);
            }
            Statement::Emit(effect) => {
                out[emitted] = Some(build_effect(effect, input, tuning, attrs, &locals));
                emitted += 1;
            }
        }
    }
    assert_eq!(emitted, N, "rule {id:?} emitted {emitted} effects, expected {N}");
    out.map(|effect| effect.expect("every effect slot filled"))
}

/// Evaluate the neutral `takeoff` selection and ordered effects.
pub fn takeoff(input: &FighterInput, tuning: &CommonTuning) -> [MechanicEffect; 4] {
    run(RuleId::Takeoff, input, tuning, None)
}

/// Evaluate the neutral `air_jump` selection, velocity and ordered effects.
pub fn air_jump(
    input: &FighterInput,
    tuning: &CommonTuning,
    attrs: &FighterAttrs,
) -> [MechanicEffect; 3] {
    run(RuleId::AirJump, input, tuning, Some(attrs))
}
