//! Deterministic fixtures: the generated Rust decisions and effect sequences
//! must agree with the source-derived rules already emitted by `smash-import`
//! (`smash/src/fighters/pigeon/generated/2_source_rules.json`).
//!
//! Every rule is evaluated with the same typed inputs the generated functions
//! receive, including the facing sign flip and a non-zero unresolved
//! `p_ftCommonData->x78`. Ordered-effect fixtures cover every source call and
//! write of the two callback translations.

use game_ftcommon::{
    CoAttrs, CommonData, FighterQuery, FtCommonEffect, FtMotionId, MotionFlags, Vec3,
    ftCo_800C97A8, ftCo_JumpAerial_Enter_Basic, ftCo_Jump_Enter,
};
use serde_json::Value;

const RULES: &str = include_str!("../../../smash/src/fighters/pigeon/generated/2_source_rules.json");
const GENERATED: &str = include_str!("../src/generated/0_ftcommon.rs");

const REVISION: &str = "c7861544f8e1fbc530612393e91d859886e97e3c";

fn side(text: &str, query: &FighterQuery, common: &CommonData) -> f32 {
    if text == "fp->input.lstick[0].x*fp->facing_dir" {
        return query.lstick_x * query.facing_dir;
    }
    if let Some(field) = text.strip_prefix("-p_ftCommonData->") {
        return -common_field(field, common);
    }
    if let Some(field) = text.strip_prefix("p_ftCommonData->") {
        return common_field(field, common);
    }
    panic!("unexpected guard side: {text}");
}

fn common_field(field: &str, common: &CommonData) -> f32 {
    match field {
        "x34" => common.x34,
        "x78" => common.x78,
        other => panic!("unexpected p_ftCommonData field: {other}"),
    }
}

fn holds(rule: &Value, query: &FighterQuery, common: &CommonData) -> bool {
    let guard = &rule["guard"];
    let lhs = side(guard["lhs"].as_str().unwrap(), query, common);
    let rhs = side(guard["rhs"].as_str().unwrap(), query, common);
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

fn expected_action(to: &str) -> FtMotionId {
    match to {
        "JumpF" => FtMotionId::JumpF,
        "JumpB" => FtMotionId::JumpB,
        "JumpAerialF" => FtMotionId::JumpAerialF,
        "JumpAerialB" => FtMotionId::JumpAerialB,
        other => panic!("unexpected action: {other}"),
    }
}

fn jump_motion(effects: [FtCommonEffect; 4]) -> FtMotionId {
    match effects[1] {
        FtCommonEffect::Fighter_ChangeMotionState { motion, .. } => motion,
        other => panic!("expected a motion change, got {other:?}"),
    }
}

fn aerial_motion(effects: [FtCommonEffect; 3]) -> FtMotionId {
    match effects[2] {
        FtCommonEffect::FtCo_800CBAC4 { motion, .. } => motion,
        other => panic!("expected an aerial launch, got {other:?}"),
    }
}

fn rules() -> Value {
    serde_json::from_str(RULES).unwrap()
}

#[test]
fn generated_source_retains_provenance_and_typed_inputs() {
    for name in ["ftCo_800C97A8", "ftCo_Jump_Enter", "ftCo_JumpAerial_Enter_Basic"] {
        assert!(GENERATED.contains(name), "missing C name {name}");
    }
    assert!(GENERATED.contains(REVISION));
    assert!(GENERATED.contains("Source callback: `ftCo_800C97A8`"));
    assert!(GENERATED.contains("Source callback: `ftCo_Jump_Enter`"));
    assert!(GENERATED.contains("Source callback: `ftCo_JumpAerial_Enter_Basic`"));
    assert!(!GENERATED.contains("github.com"));
    // Unresolved fields and attributes stay typed inputs named after the C fields.
    assert!(GENERATED.contains("common.x34"));
    assert!(GENERATED.contains("common.x78"));
    assert!(GENERATED.contains("attrs.air_jump_h_multiplier"));
    assert!(GENERATED.contains("p_ftCommonData->x34"));
    assert!(GENERATED.contains("p_ftCommonData->x78"));
    // Opaque effect variants keep the decomp symbol.
    assert!(GENERATED.contains("FtCommonEffect::ftCommon_8007D5D4"));
    assert!(GENERATED.contains("FtCommonEffect::FtCo_800CB110"));
    assert!(GENERATED.contains("FtCommonEffect::FtCo_800CBAC4"));
    assert!(GENERATED.contains("FtCommonEffect::WriteX2227B0"));
    assert!(GENERATED.contains("FtCommonEffect::WriteCmdVars0"));
}

#[test]
fn generated_decisions_match_emitted_rules() {
    let rules = rules();
    assert_eq!(rules["revision"].as_str().unwrap(), REVISION);
    assert_eq!(rules["rules"].as_array().unwrap().len(), 5);

    let cases = [
        (FighterQuery { lstick_x: 1.0, facing_dir: 1.0 }, CommonData { x34: 0.0, x78: 0.5 }),
        (FighterQuery { lstick_x: -1.0, facing_dir: 1.0 }, CommonData { x34: 0.0, x78: 0.5 }),
        (FighterQuery { lstick_x: 1.0, facing_dir: -1.0 }, CommonData { x34: 0.0, x78: 0.5 }),
        (FighterQuery { lstick_x: 0.25, facing_dir: 1.0 }, CommonData { x34: 0.0, x78: 0.5 }),
    ];
    let attrs = CoAttrs {
        air_jump_h_multiplier: 0.8,
        jump_v_initial_velocity: 3.2,
        air_jump_v_multiplier: 0.9,
    };

    for (query, common) in cases {
        for rule in rules["rules"].as_array().unwrap() {
            let symbol = rule["source"]["symbol"].as_str().unwrap();
            let to = rule["to"].as_str().unwrap();
            let holds = holds(rule, &query, &common);
            match symbol {
                "ftCo_800C97A8" => {
                    assert_eq!(holds, ftCo_800C97A8(&query, &common), "to={to}");
                }
                "ftCo_Jump_Enter" => {
                    assert_eq!(
                        holds,
                        jump_motion(ftCo_Jump_Enter(&query, &common)) == expected_action(to),
                        "to={to}",
                    );
                }
                "ftCo_JumpAerial_Enter_Basic" => {
                    assert_eq!(
                        holds,
                        aerial_motion(ftCo_JumpAerial_Enter_Basic(&query, &common, &attrs))
                            == expected_action(to),
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
    let query = FighterQuery { lstick_x: 1.0, facing_dir: 1.0 };
    let common = CommonData { x34: 0.0, x78: 0.5 };
    let attrs = CoAttrs {
        air_jump_h_multiplier: 0.8,
        jump_v_initial_velocity: 3.2,
        air_jump_v_multiplier: 0.9,
    };

    assert_eq!(
        ftCo_Jump_Enter(&query, &common),
        [
            FtCommonEffect::ftCommon_8007D5D4,
            FtCommonEffect::Fighter_ChangeMotionState {
                motion: FtMotionId::JumpF,
                flags: MotionFlags::None,
                anim_start: 0.0,
                anim_speed: 1.0,
                anim_blend: 0.0,
            },
            FtCommonEffect::FtCo_800CB110 { arg1: true, jump_mul: 1.0 },
            FtCommonEffect::WriteX2227B0 { value: true },
        ],
    );

    let velocity = Vec3 {
        x: query.lstick_x * attrs.air_jump_h_multiplier,
        y: attrs.jump_v_initial_velocity * attrs.air_jump_v_multiplier,
        z: 0.0,
    };
    assert_eq!(
        ftCo_JumpAerial_Enter_Basic(&query, &common, &attrs),
        [
            FtCommonEffect::ftCommon_8007D5D4,
            FtCommonEffect::WriteCmdVars0 { value: 1 },
            FtCommonEffect::FtCo_800CBAC4 {
                motion: FtMotionId::JumpAerialF,
                velocity,
                arg3: true,
            },
        ],
    );
}

#[test]
fn computed_aerial_velocity_uses_typed_attributes() {
    let query = FighterQuery { lstick_x: 0.5, facing_dir: 1.0 };
    let common = CommonData { x34: 0.0, x78: 0.0 };
    let attrs = CoAttrs {
        air_jump_h_multiplier: 0.8,
        jump_v_initial_velocity: 3.2,
        air_jump_v_multiplier: 0.9,
    };
    let effects = ftCo_JumpAerial_Enter_Basic(&query, &common, &attrs);
    let FtCommonEffect::FtCo_800CBAC4 { velocity, .. } = effects[2] else {
        panic!("expected an aerial launch");
    };
    assert_eq!(velocity.x, 0.5 * 0.8);
    assert_eq!(velocity.y, 3.2 * 0.9);
    assert_eq!(velocity.z, 0.0);
    assert_ne!(velocity.y, velocity.x);
}

#[test]
fn facing_sign_flips_the_selection() {
    let common = CommonData { x34: 0.0, x78: 0.0 };
    let attrs = CoAttrs {
        air_jump_h_multiplier: 1.0,
        jump_v_initial_velocity: 1.0,
        air_jump_v_multiplier: 1.0,
    };
    let right = FighterQuery { lstick_x: 1.0, facing_dir: 1.0 };
    let left = FighterQuery { lstick_x: 1.0, facing_dir: -1.0 };
    assert_eq!(jump_motion(ftCo_Jump_Enter(&right, &common)), FtMotionId::JumpF);
    assert_eq!(jump_motion(ftCo_Jump_Enter(&left, &common)), FtMotionId::JumpB);
    assert_eq!(
        aerial_motion(ftCo_JumpAerial_Enter_Basic(&right, &common, &attrs)),
        FtMotionId::JumpAerialF,
    );
    assert_eq!(
        aerial_motion(ftCo_JumpAerial_Enter_Basic(&left, &common, &attrs)),
        FtMotionId::JumpAerialB,
    );
    assert!(ftCo_800C97A8(&left, &common));
    assert!(!ftCo_800C97A8(&right, &common));
}

#[test]
fn unresolved_common_data_feeds_the_boundary() {
    // The same stick value crosses the source `-p_ftCommonData->x78` threshold
    // when the caller supplies a different retained value.
    let query = FighterQuery { lstick_x: 0.25, facing_dir: 1.0 };
    let low = CommonData { x34: 0.0, x78: 0.5 };
    let high = CommonData { x34: 0.0, x78: -0.25 };
    assert_eq!(jump_motion(ftCo_Jump_Enter(&query, &low)), FtMotionId::JumpF);
    assert_eq!(jump_motion(ftCo_Jump_Enter(&query, &high)), FtMotionId::JumpB);
    assert!(ftCo_800C97A8(&query, &CommonData { x34: 0.25, x78: 0.0 }));
    assert!(!ftCo_800C97A8(&query, &CommonData { x34: 0.0, x78: 0.0 }));
}
