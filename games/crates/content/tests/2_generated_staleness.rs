#![cfg(feature = "ingest")]

use game_content::{
    Op, PortBody, PortEffect, PortExpr, PortFile, PortFn, PortSpan, PortStatement, PortValue,
    emit_port_rust,
};

const GENERATED: &str = include_str!("../../ftcommon/src/generated/0_ftcommon.rs");

fn compare(neg: bool) -> PortExpr {
    PortExpr::Compare(
        Box::new(PortExpr::Mul(
            Box::new(PortExpr::Value(PortValue::Input("StickX"))),
            Box::new(PortExpr::Value(PortValue::Input("Facing"))),
        )),
        if neg { Op::Greater } else { Op::LessEqual },
        Box::new(if neg {
            PortExpr::Neg(Box::new(PortExpr::Value(PortValue::Tuning(
                "JumpBackThreshold",
            ))))
        } else {
            PortExpr::Value(PortValue::Tuning("TurnThreshold"))
        }),
    )
}

fn select(fwd: &'static str, back: &'static str) -> PortExpr {
    PortExpr::Conditional {
        condition: Box::new(compare(true)),
        yes: Box::new(PortExpr::Value(PortValue::Motion(fwd))),
        no: Box::new(PortExpr::Value(PortValue::Motion(back))),
    }
}

fn vector() -> PortExpr {
    PortExpr::Vector {
        x: Box::new(PortExpr::Mul(
            Box::new(PortExpr::Value(PortValue::Input("StickX"))),
            Box::new(PortExpr::Value(PortValue::Attr("AirJumpHScale"))),
        )),
        y: Box::new(PortExpr::Mul(
            Box::new(PortExpr::Value(PortValue::Attr("JumpInitialSpeed"))),
            Box::new(PortExpr::Value(PortValue::Attr("AirJumpVScale"))),
        )),
        z: Box::new(PortExpr::Value(PortValue::Float(0.0))),
    }
}

#[test]
fn committed_generated_file_matches_emitter() {
    let functions = vec![
        PortFn {
            name: "ftCo_800C97A8".into(),
            path: "src/melee/ft/kinds/ftCommon/ftCo_Turn.c".into(),
            function_line: 28,
            function_end_line: 36,
            span: PortSpan { line: 32, column: 8, end_line: 32, end_column: 71 },
            calls: vec![],
            body: PortBody::Guard(compare(false)),
        },
        PortFn {
            name: "ftCo_Jump_Enter".into(),
            path: "src/melee/ft/kinds/ftCommon/ftCo_Jump.c".into(),
            function_line: 153,
            function_end_line: 165,
            span: PortSpan { line: 153, column: 1, end_line: 165, end_column: 2 },
            calls: vec![
                "ftCommon_8007D5D4".into(),
                "Fighter_ChangeMotionState".into(),
                "ftCo_800CB110".into(),
            ],
            body: PortBody::Callback(vec![
                PortStatement::Effect(PortEffect::BeginJump),
                PortStatement::Bind { name: "Motion".into(), expr: select("JumpForward", "JumpBackward") },
                PortStatement::Effect(PortEffect::EnterMotion {
                    motion: PortExpr::Value(PortValue::Local("Motion")),
                    flags: "None",
                    anim_start: 0.0,
                    anim_speed: 1.0,
                    anim_blend: 0.0,
                }),
                PortStatement::Effect(PortEffect::SetJumpParams {
                    enabled: PortExpr::Value(PortValue::Bool(true)),
                    scale: 1.0,
                }),
                PortStatement::Effect(PortEffect::SetFlag {
                    value: PortExpr::Value(PortValue::Bool(true)),
                }),
            ]),
        },
        PortFn {
            name: "ftCo_JumpAerial_Enter_Basic".into(),
            path: "src/melee/ft/kinds/ftCommon/ftCo_JumpAerial.c".into(),
            function_line: 158,
            function_end_line: 177,
            span: PortSpan { line: 158, column: 1, end_line: 177, end_column: 2 },
            calls: vec!["ftCommon_8007D5D4".into(), "ftCo_800CBAC4".into()],
            body: PortBody::Callback(vec![
                PortStatement::Effect(PortEffect::BeginJump),
                PortStatement::Effect(PortEffect::SetCommandValue {
                    slot: 0,
                    value: PortExpr::Value(PortValue::Unsigned(1)),
                }),
                PortStatement::Bind { name: "Motion".into(), expr: select("AirJumpForward", "AirJumpBackward") },
                PortStatement::Bind { name: "Velocity".into(), expr: vector() },
                PortStatement::Effect(PortEffect::LaunchJump {
                    motion: PortExpr::Value(PortValue::Local("Motion")),
                    velocity: "Velocity",
                    flag: PortExpr::Value(PortValue::Bool(true)),
                }),
            ]),
        },
    ];
    let emitted = emit_port_rust(&PortFile {
        repository: "https://github.com/doldecomp/melee.git",
        revision: "c7861544f8e1fbc530612393e91d859886e97e3c",
        functions: &functions,
    })
    .unwrap();
    if emitted != GENERATED {
        std::fs::write("/tmp/emitted_ftcommon.rs", &emitted).unwrap();
        panic!("committed generated file differs from emitter output; see /tmp/emitted_ftcommon.rs");
    }
}
