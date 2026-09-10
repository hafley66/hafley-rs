use game_content::{
    Guard, Op, SourceRef, SourceRule, TransitionSpec, Trigger, Unresolved,
    conditional_choice, decode_file, emit_chart, function_evidence, if_guard,
};
use serde::Serialize;
use std::path::{Path, PathBuf};

const MELEE_REPOSITORY: &str = "https://github.com/doldecomp/melee.git";
const TURN_PATH: &str = "src/melee/ft/kinds/ftCommon/ftCo_Turn.c";
const JUMP_PATH: &str = "src/melee/ft/kinds/ftCommon/ftCo_Jump.c";
const AIR_JUMP_PATH: &str = "src/melee/ft/kinds/ftCommon/ftCo_JumpAerial.c";

#[derive(Serialize)]
struct SourceImport {
    repository: &'static str,
    revision: String,
    rules: Vec<SourceRule>,
    unresolved: Vec<Unresolved>,
}

fn falcon_source() -> Result<String, Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/fighters/falcon/imported");
    let files = [
        "Wait1.html", "JumpF.html", "AttackAirF.html",
        "JumpSquat.html", "Fall.html", "LandingAirF.html",
        "LandingHeavy.html",
    ];
    let actions = files.iter().map(|file| decode_file(&root.join(file)))
        .collect::<Result<Vec<_>, _>>()?;
    let transitions = [
        TransitionSpec { from: "JumpSquat", trigger: Trigger::Complete, to: "JumpF" },
        TransitionSpec { from: "JumpF", trigger: Trigger::Complete, to: "Fall" },
        TransitionSpec { from: "AttackAirF", trigger: Trigger::Complete, to: "Fall" },
        TransitionSpec { from: "LandingAirF", trigger: Trigger::Complete, to: "Wait1" },
        TransitionSpec { from: "LandingHeavy", trigger: Trigger::Complete, to: "Wait1" },
        TransitionSpec { from: "Wait1", trigger: Trigger::JumpPress, to: "JumpSquat" },
        TransitionSpec { from: "JumpF", trigger: Trigger::AttackEligible, to: "AttackAirF" },
        TransitionSpec { from: "Fall", trigger: Trigger::AttackEligible, to: "AttackAirF" },
        TransitionSpec { from: "AttackAirF", trigger: Trigger::AttackEligible, to: "AttackAirF" },
        TransitionSpec { from: "AttackAirF", trigger: Trigger::LandDuringAttack, to: "LandingAirF" },
        TransitionSpec { from: "JumpF", trigger: Trigger::Land, to: "LandingHeavy" },
        TransitionSpec { from: "Fall", trigger: Trigger::Land, to: "LandingHeavy" },
    ];
    Ok(emit_chart(&actions, &transitions)?)
}

fn falcon(output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::write(output, falcon_source()?)?;
    std::fs::write(output.with_file_name("1_attributes.rs"), attribute_source()?)?;
    let source = source_import()?;
    std::fs::write(
        output.with_file_name("2_source_rules.json"),
        format!("{}\n", serde_json::to_string_pretty(&source)?),
    )?;
    std::fs::write(output.with_file_name("2_source_rules.rs"), source_rust(&source))?;
    std::fs::write(output.with_file_name("3_source_chart.d2"), source_chart(&source))?;
    Ok(())
}

fn falcon_check(output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let source = source_import()?;
    let expected = [
        (output.to_path_buf(), falcon_source()?),
        (output.with_file_name("1_attributes.rs"), attribute_source()?),
        (
            output.with_file_name("2_source_rules.json"),
            format!("{}\n", serde_json::to_string_pretty(&source)?),
        ),
        (output.with_file_name("2_source_rules.rs"), source_rust(&source)),
        (output.with_file_name("3_source_chart.d2"), source_chart(&source)),
    ];
    for (path, expected) in expected {
        if std::fs::read_to_string(&path)? != expected {
            return Err(format!("stale generated output: {}", path.display()).into());
        }
    }
    Ok(())
}

fn melee_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../vendor/melee")
}

fn revision(root: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let output = std::process::Command::new("git")
        .args(["-C", root.to_str().ok_or("non-UTF8 source path")?, "rev-parse", "HEAD"])
        .output()?;
    if !output.status.success() {
        return Err("cannot read pinned Melee submodule revision".into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().into())
}

fn source_ref(revision: &str, path: &str, line: usize, symbol: &str) -> SourceRef {
    SourceRef {
        repository: MELEE_REPOSITORY.into(),
        revision: revision.into(),
        path: path.into(),
        line,
        symbol: symbol.into(),
    }
}

fn inverse(guard: &Guard) -> Guard {
    Guard {
        lhs: guard.lhs.clone(),
        op: match guard.op {
            Op::Less => Op::GreaterEqual,
            Op::LessEqual => Op::Greater,
            Op::Greater => Op::LessEqual,
            Op::GreaterEqual => Op::Less,
            Op::Equal => Op::NotEqual,
            Op::NotEqual => Op::Equal,
        },
        rhs: guard.rhs.clone(),
    }
}

fn choice_rules(
    source: &str,
    revision: &str,
    path: &str,
    function: &str,
    from: &str,
    event: &str,
) -> Result<Vec<SourceRule>, Box<dyn std::error::Error>> {
    let (guard, yes, no, line) = conditional_choice(source, function)?;
    Ok(vec![
        SourceRule {
            from: from.into(),
            event: event.into(),
            to: yes.trim_start_matches("ftCo_MS_").into(),
            guard: guard.clone(),
            source: source_ref(revision, path, line, function),
        },
        SourceRule {
            from: from.into(),
            event: event.into(),
            to: no.trim_start_matches("ftCo_MS_").into(),
            guard: inverse(&guard),
            source: source_ref(revision, path, line, function),
        },
    ])
}

fn source_import() -> Result<SourceImport, Box<dyn std::error::Error>> {
    let root = melee_root();
    let revision = revision(&root)?;
    let turn = std::fs::read_to_string(root.join(TURN_PATH))?;
    let jump = std::fs::read_to_string(root.join(JUMP_PATH))?;
    let air_jump = std::fs::read_to_string(root.join(AIR_JUMP_PATH))?;

    let (turn_guard, turn_line) = if_guard(&turn, "ftCo_800C97A8")?;
    let turn_calls = function_evidence(&turn, "ftCo_Turn_CheckInput")?;
    for required in ["ftCo_800C97A8", "ftCo_Turn_Enter_Basic"] {
        if !turn_calls.calls.iter().any(|call| call == required) {
            return Err(format!("ftCo_Turn_CheckInput does not call {required}").into());
        }
    }

    let mut rules = vec![SourceRule {
        from: "Wait".into(),
        event: "turn_request".into(),
        to: "Turn".into(),
        guard: turn_guard,
        source: source_ref(&revision, TURN_PATH, turn_line, "ftCo_800C97A8"),
    }];
    rules.extend(choice_rules(
        &jump,
        &revision,
        JUMP_PATH,
        "ftCo_Jump_Enter",
        "KneeBend",
        "takeoff",
    )?);
    rules.extend(choice_rules(
        &air_jump,
        &revision,
        AIR_JUMP_PATH,
        "ftCo_JumpAerial_Enter_Basic",
        "Fall",
        "air_jump",
    )?);

    let unresolved = [
        ("p_ftCommonData->x34", TURN_PATH, "ftCo_800C97A8"),
        ("p_ftCommonData->x78", JUMP_PATH, "ftCo_Jump_Enter"),
    ]
    .into_iter()
    .map(|(symbol, path, owner)| Unresolved {
        symbol: symbol.into(),
        source: source_ref(&revision, path, 0, owner),
        reason: "numeric value lives in the game common-data binary; no retained DAT input".into(),
    })
    .chain([Unresolved {
        symbol: "LandingLight selection".into(),
        source: SourceRef {
            repository: "https://rukaidata.com/PM3.6/Captain%20Falcon/subactions/".into(),
            revision: "7726bc3c985636a9bf5f6a64b52be9af38b177d5e6f7f63b8f5d1c6204aa7a75".into(),
            path: "LandingLight.html".into(),
            line: 0,
            symbol: "LandingLight".into(),
        },
        reason: "payload establishes the action; no retained PM selection rule".into(),
    }])
    .collect();

    Ok(SourceImport {
        repository: MELEE_REPOSITORY,
        revision,
        rules,
        unresolved,
    })
}

fn op(op: Op) -> &'static str {
    match op {
        Op::Less => "<",
        Op::LessEqual => "<=",
        Op::Greater => ">",
        Op::GreaterEqual => ">=",
        Op::Equal => "==",
        Op::NotEqual => "!=",
    }
}

fn source_chart(import: &SourceImport) -> String {
    let mut output = String::from(
        "# Generated by smash-import from pinned source syntax trees.\n\ndirection: right\nclasses: {\n  unresolved: { style: { stroke: \"#eab308\"; stroke-dash: 4 } }\n}\n",
    );
    for rule in &import.rules {
        output.push_str(&format!(
            "\"{}\" -> \"{}\": \"{} [{} {} {}]\"\n",
            rule.from,
            rule.to,
            rule.event,
            rule.guard.lhs.replace('"', "\\\""),
            op(rule.guard.op),
            rule.guard.rhs.replace('"', "\\\""),
        ));
    }
    for (index, item) in import.unresolved.iter().enumerate() {
        output.push_str(&format!(
            "unresolved_{index}: \"UNRESOLVED: {}\" {{ class: unresolved }}\n",
            item.symbol.replace('"', "\\\""),
        ));
    }
    output
}

fn source_rust(import: &SourceImport) -> String {
    let mut output = String::from(
        "// Generated by smash-import from pinned source syntax trees. Do not edit.\n\nuse game_content::{Guard, Op, SourceRef, SourceRule, Unresolved};\n\n",
    );
    output.push_str("pub fn rules() -> Vec<SourceRule> {\n    vec![\n");
    for rule in &import.rules {
        output.push_str(&format!(
            "        SourceRule {{ from: {:?}.into(), event: {:?}.into(), to: {:?}.into(), guard: Guard {{ lhs: {:?}.into(), op: Op::{:?}, rhs: {:?}.into() }}, source: SourceRef {{ repository: {:?}.into(), revision: {:?}.into(), path: {:?}.into(), line: {}, symbol: {:?}.into() }} }},\n",
            rule.from,
            rule.event,
            rule.to,
            rule.guard.lhs,
            rule.guard.op,
            rule.guard.rhs,
            rule.source.repository,
            rule.source.revision,
            rule.source.path,
            rule.source.line,
            rule.source.symbol,
        ));
    }
    output.push_str("    ]\n}\n\npub fn unresolved() -> Vec<Unresolved> {\n    vec![\n");
    for item in &import.unresolved {
        output.push_str(&format!(
            "        Unresolved {{ symbol: {:?}.into(), source: SourceRef {{ repository: {:?}.into(), revision: {:?}.into(), path: {:?}.into(), line: {}, symbol: {:?}.into() }}, reason: {:?}.into() }},\n",
            item.symbol,
            item.source.repository,
            item.source.revision,
            item.source.path,
            item.source.line,
            item.source.symbol,
            item.reason,
        ));
    }
    output.push_str("    ]\n}\n");
    output
}

fn attribute_source() -> Result<String, Box<dyn std::error::Error>> {
    let html = include_str!("fighters/falcon/imported/attributes.html");
    let values = game_content::attributes(html)?;
    let mut output = String::from("// Generated by smash-import from retained PM3.6 attributes.html.\n#![allow(dead_code)]\n\n");
    for (name, value) in values {
        let ident = name.replace(' ', "_").to_uppercase();
        if !ident.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') {
            return Err(format!("invalid attribute identifier {ident}").into());
        }
        output.push_str(&format!("pub const {ident}: f32 = {value:?};\n"));
    }
    Ok(output)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let fighter = args.next().ok_or("usage: smash-import falcon [--check] [output]")?;
    let second = args.next();
    let check = second.as_deref() == Some(std::ffi::OsStr::new("--check"));
    let output_arg = if check { args.next() } else { second };
    if fighter != "falcon" || args.next().is_some() {
        return Err("usage: smash-import falcon [--check] [output]".into());
    }
    let output = output_arg.map(PathBuf::from).unwrap_or_else(|| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/fighters/falcon/generated/0_chart.rs")
    });
    if check { falcon_check(&output) } else { falcon(&output) }
}

#[cfg(test)]
mod tests {
    #[test]
    fn generated_falcon_chart_is_current() {
        assert_eq!(
            super::falcon_source().unwrap(),
            include_str!("fighters/falcon/generated/0_chart.rs"),
        );
    }

    #[test]
    fn generated_attributes_are_current() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/fighters/falcon/generated/1_attributes.rs");
        assert_eq!(super::attribute_source().unwrap(), std::fs::read_to_string(path).unwrap());
    }

    #[test]
    fn generated_source_rules_are_current() {
        let import = super::source_import().unwrap();
        let json = format!("{}\n", serde_json::to_string_pretty(&import).unwrap());
        assert_eq!(json, include_str!("fighters/falcon/generated/2_source_rules.json"));
        assert_eq!(
            super::source_chart(&import),
            include_str!("fighters/falcon/generated/3_source_chart.d2"),
        );
        assert_eq!(
            super::source_rust(&import),
            include_str!("fighters/falcon/generated/2_source_rules.rs"),
        );
    }

    #[test]
    fn source_rules_preserve_unknown_values() {
        let import = super::source_import().unwrap();
        assert_eq!(import.rules.len(), 5);
        assert_eq!(
            import.unresolved.iter().map(|item| item.symbol.as_str()).collect::<Vec<_>>(),
            ["p_ftCommonData->x34", "p_ftCommonData->x78", "LandingLight selection"],
        );
    }
}
