//! Deterministic fixtures: the generated neutral rule data and the authored
//! evaluator must agree with the source-derived rules already emitted by
//! `smash-import` (`smash/src/fighters/pigeon/generated/2_source_rules.json`).
//!
//! Every rule is evaluated with the same typed inputs the evaluator receives,
//! including the facing sign flip and a non-zero unresolved
//! `p_ftCommonData->x78`. Ordered-effect fixtures cover every source call and
//! write of the two callback translations. The generated module keeps source
//! identities under provenance; its runtime declarations stay neutral.

use game_ftcommon::{
    CommonTuning, FighterAttrs, FighterInput, MechanicEffect, Motion, MotionFlags, RULES, RuleId,
    Vec3, air_jump, takeoff, turn_request,
};
use serde_json::Value;

const RULES_JSON: &str = include_str!("../../../smash/src/fighters/pigeon/generated/2_source_rules.json");
const GENERATED: &str = include_str!("../src/generated/0_ftcommon.rs");

const REVISION: &str = "c7861544f8e1fbc530612393e91d859886e97e3c";

fn side(text: &str, input: &FighterInput, tuning: &CommonTuning) -> f32 {
    if text == "fp->input.lstick[0].x*fp->facing_dir" {
        return input.stick_x * input.facing;
    }
    if let Some(field) = text.strip_prefix("-p_ftCommonData->") {
        return -tuning_field(field, tuning);
    }
    if let Some(field) = text.strip_prefix("p_ftCommonData->") {
        return tuning_field(field, tuning);
    }
    panic!("unexpected guard side: {text}");
}

fn tuning_field(field: &str, tuning: &CommonTuning) -> f32 {
    match field {
        "x34" => tuning.turn_threshold,
        "x78" => tuning.jump_back_threshold,
        other => panic!("unexpected p_ftCommonData field: {other}"),
    }
}

fn holds(rule: &Value, input: &FighterInput, tuning: &CommonTuning) -> bool {
    let guard = &rule["guard"];
    let lhs = side(guard["lhs"].as_str().unwrap(), input, tuning);
    let rhs = side(guard["rhs"].as_str().unwrap(), input, tuning);
    match guard["operator"].as_str().unwrap() {
        "Less" => lhs < rhs,
        "LessEqual" => lhs <= rhs,
        "Greater" => lhs > rhs,
        "GreaterEqual" => lhs >= rhs,
        "Equal" => lhs == rhs,
        "NotEqual" => lhs != rhs,
        other => panic!("unexpected operator: {other}"),
    }
}

fn expected_action(to: &str) -> Motion {
    match to {
        "JumpF" => Motion::JumpForward,
        "JumpB" => Motion::JumpBackward,
        "JumpAerialF" => Motion::AirJumpForward,
        "JumpAerialB" => Motion::AirJumpBackward,
        other => panic!("unexpected action: {other}"),
    }
}

fn jump_motion(effects: [MechanicEffect; 4]) -> Motion {
    match effects[1] {
        MechanicEffect::EnterMotion { motion, .. } => motion,
        other => panic!("expected a motion change, got {other:?}"),
    }
}

fn aerial_motion(effects: [MechanicEffect; 3]) -> Motion {
    match effects[2] {
        MechanicEffect::LaunchJump { motion, .. } => motion,
        other => panic!("expected an aerial launch, got {other:?}"),
    }
}

fn rules() -> Value {
    serde_json::from_str(RULES_JSON).unwrap()
}

#[test]
fn generated_rule_data_retains_provenance_and_stays_neutral() {
    // Source identities survive under provenance.
    for symbol in [
        "ftCo_800C97A8",
        "ftCo_Jump_Enter",
        "ftCo_JumpAerial_Enter_Basic",
        "ftCommon_8007D5D4",
        "Fighter_ChangeMotionState",
        "ftCo_800CB110",
        "ftCo_800CBAC4",
        "p_ftCommonData->x34",
        "p_ftCommonData->x78",
    ] {
        assert!(GENERATED.contains(symbol), "missing source identity {symbol}");
    }
    assert!(GENERATED.contains(REVISION));
    assert!(!GENERATED.contains("github.com"));
    // Runtime declarations are neutral.
    assert!(GENERATED.contains("pub static RULES: &[Rule] = &["));
    for id in ["TurnRequest", "Takeoff", "AirJump"] {
        assert!(GENERATED.contains(&format!("id: RuleId::{id}")));
        assert!(RULES.iter().any(|rule| format!("{rule:?}").contains(id)));
    }
    assert_eq!(RULES.len(), 3);
    assert!(RULES.iter().all(|rule| rule.id != RuleId::TurnRequest || rule.guard.is_some()));
    assert!(RULES.iter().all(|rule| rule.id == RuleId::TurnRequest || rule.guard.is_none()));
    for banned in [
        "pub fn ftCo_",
        "FtCommonEffect",
        "FtMotionId",
        "CommonData {",
        ": &CommonData",
        "CoAttrs",
        "FighterQuery",
    ] {
        assert!(!GENERATED.contains(banned), "runtime declaration not neutral: {banned}");
    }
}

#[test]
fn generated_decisions_match_emitted_rules() {
    let rules = rules();
    assert_eq!(rules["revision"].as_str().unwrap(), REVISION);
    assert_eq!(rules["rules"].as_array().unwrap().len(), 5);

    let cases = [
        (FighterInput { stick_x: 1.0, facing: 1.0 }, CommonTuning { turn_threshold: 0.0, jump_back_threshold: 0.5 }),
        (FighterInput { stick_x: -1.0, facing: 1.0 }, CommonTuning { turn_threshold: 0.0, jump_back_threshold: 0.5 }),
        (FighterInput { stick_x: 1.0, facing: -1.0 }, CommonTuning { turn_threshold: 0.0, jump_back_threshold: 0.5 }),
        (FighterInput { stick_x: 0.25, facing: 1.0 }, CommonTuning { turn_threshold: 0.0, jump_back_threshold: 0.5 }),
    ];
    let attrs = FighterAttrs {
        air_jump_h_scale: 0.8,
        jump_initial_speed: 3.2,
        air_jump_v_scale: 0.9,
    };

    for (input, tuning) in cases {
        for rule in rules["rules"].as_array().unwrap() {
            let symbol = rule["source"]["symbol"].as_str().unwrap();
            let to = rule["to"].as_str().unwrap();
            let holds = holds(rule, &input, &tuning);
            match symbol {
                "ftCo_800C97A8" => {
                    assert_eq!(holds, turn_request(&input, &tuning), "to={to}");
                }
                "ftCo_Jump_Enter" => {
                    assert_eq!(
                        holds,
                        jump_motion(takeoff(&input, &tuning)) == expected_action(to),
                        "to={to}",
                    );
                }
                "ftCo_JumpAerial_Enter_Basic" => {
                    assert_eq!(
                        holds,
                        aerial_motion(air_jump(&input, &tuning, &attrs)) == expected_action(to),
                        "to={to}",
                    );
                }
                other => panic!("unexpected source symbol: {other}"),
            }
        }
    }
}

#[test]
fn ordered_effects_cover_every_source_call_and_write() {
    let input = FighterInput { stick_x: 1.0, facing: 1.0 };
    let tuning = CommonTuning { turn_threshold: 0.0, jump_back_threshold: 0.5 };
    let attrs = FighterAttrs {
        air_jump_h_scale: 0.8,
        jump_initial_speed: 3.2,
        air_jump_v_scale: 0.9,
    };

    assert_eq!(
        takeoff(&input, &tuning),
        [
            MechanicEffect::BeginJump,
            MechanicEffect::EnterMotion {
                motion: Motion::JumpForward,
                flags: MotionFlags::None,
                anim_start: 0.0,
                anim_speed: 1.0,
                anim_blend: 0.0,
            },
            MechanicEffect::SetJumpParams { enabled: true, scale: 1.0 },
            MechanicEffect::SetFlag { value: true },
        ],
    );

    let velocity = Vec3 {
        x: input.stick_x * attrs.air_jump_h_scale,
        y: attrs.jump_initial_speed * attrs.air_jump_v_scale,
        z: 0.0,
    };
    assert_eq!(
        air_jump(&input, &tuning, &attrs),
        [
            MechanicEffect::BeginJump,
            MechanicEffect::SetCommandValue { slot: 0, value: 1 },
            MechanicEffect::LaunchJump {
                motion: Motion::AirJumpForward,
                velocity,
                flag: true,
            },
        ],
    );
}

#[test]
fn computed_aerial_velocity_uses_typed_attributes() {
    let input = FighterInput { stick_x: 0.5, facing: 1.0 };
    let tuning = CommonTuning { turn_threshold: 0.0, jump_back_threshold: 0.0 };
    let attrs = FighterAttrs {
        air_jump_h_scale: 0.8,
        jump_initial_speed: 3.2,
        air_jump_v_scale: 0.9,
    };
    let effects = air_jump(&input, &tuning, &attrs);
    let MechanicEffect::LaunchJump { velocity, .. } = effects[2] else {
        panic!("expected an aerial launch");
    };
    assert_eq!(velocity.x, 0.5 * 0.8);
    assert_eq!(velocity.y, 3.2 * 0.9);
    assert_eq!(velocity.z, 0.0);
    assert_ne!(velocity.y, velocity.x);
}

#[test]
fn facing_sign_flips_the_selection() {
    let tuning = CommonTuning { turn_threshold: 0.0, jump_back_threshold: 0.0 };
    let attrs = FighterAttrs {
        air_jump_h_scale: 1.0,
        jump_initial_speed: 1.0,
        air_jump_v_scale: 1.0,
    };
    let right = FighterInput { stick_x: 1.0, facing: 1.0 };
    let left = FighterInput { stick_x: 1.0, facing: -1.0 };
    assert_eq!(jump_motion(takeoff(&right, &tuning)), Motion::JumpForward);
    assert_eq!(jump_motion(takeoff(&left, &tuning)), Motion::JumpBackward);
    assert_eq!(
        aerial_motion(air_jump(&right, &tuning, &attrs)),
        Motion::AirJumpForward,
    );
    assert_eq!(
        aerial_motion(air_jump(&left, &tuning, &attrs)),
        Motion::AirJumpBackward,
    );
    assert!(turn_request(&left, &tuning));
    assert!(!turn_request(&right, &tuning));
}

#[test]
fn unresolved_tuning_feeds_the_boundary() {
    // The same stick value crosses the source `-p_ftCommonData->x78` threshold
    // when the caller supplies a different retained value.
    let input = FighterInput { stick_x: 0.25, facing: 1.0 };
    let low = CommonTuning { turn_threshold: 0.0, jump_back_threshold: 0.5 };
    let high = CommonTuning { turn_threshold: 0.0, jump_back_threshold: -0.25 };
    assert_eq!(jump_motion(takeoff(&input, &low)), Motion::JumpForward);
    assert_eq!(jump_motion(takeoff(&input, &high)), Motion::JumpBackward);
    assert!(turn_request(
        &input,
        &CommonTuning { turn_threshold: 0.25, jump_back_threshold: 0.0 },
    ));
    assert!(!turn_request(
        &input,
        &CommonTuning { turn_threshold: 0.0, jump_back_threshold: 0.0 },
    ));
}
