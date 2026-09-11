use game_content::{
    Guard, Inventory, Op, PortFile, RECOGNIZED_OPERATIONS, SourceRef, SourceRule, TransitionSpec,
    Trigger, Unresolved, common_inventory, conditional_choice, decode_file, emit_chart,
    emit_port_rust, function_evidence, if_guard, lower_callback, lower_guard,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const MELEE_REPOSITORY: &str = "https://github.com/doldecomp/melee.git";
const TURN_PATH: &str = "src/melee/ft/kinds/ftCommon/ftCo_Turn.c";
const JUMP_PATH: &str = "src/melee/ft/kinds/ftCommon/ftCo_Jump.c";
const AIR_JUMP_PATH: &str = "src/melee/ft/kinds/ftCommon/ftCo_JumpAerial.c";
const FTCOMMON_PATH: &str = "src/melee/ft/kinds/ftCommon";
const FTCOMMON_GENERATED: &str = "../crates/ftcommon/src/generated/0_ftcommon.rs";

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
    std::fs::write(output.with_file_name("3_source_chart.d2"), source_chart(&source))?;
    std::fs::write(ftcommon_path(), ftcommon_port(&melee_root())?)?;
    let (common, _) = common_inventory_record(&melee_root())?;
    std::fs::write(
        output.with_file_name("4_common_inventory.json"),
        format!("{}\n", serde_json::to_string(&common)?),
    )?;
    std::fs::write(
        output.with_file_name("4_common_inventory.d2"),
        common_inventory_chart(&common),
    )?;
    Ok(())
}

fn falcon_check(output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let source = source_import()?;
    let (common, _) = common_inventory_record(&melee_root())?;
    let expected = [
        (output.to_path_buf(), falcon_source()?),
        (output.with_file_name("1_attributes.rs"), attribute_source()?),
        (
            output.with_file_name("2_source_rules.json"),
            format!("{}\n", serde_json::to_string_pretty(&source)?),
        ),
        (output.with_file_name("3_source_chart.d2"), source_chart(&source)),
        (ftcommon_path(), ftcommon_port(&melee_root())?),
        (
            output.with_file_name("4_common_inventory.json"),
            format!("{}\n", serde_json::to_string(&common)?),
        ),
        (output.with_file_name("4_common_inventory.d2"), common_inventory_chart(&common)),
    ];
    for (path, expected) in expected {
        if std::fs::read_to_string(&path)? != expected {
            return Err(format!("stale generated output: {}", path.display()).into());
        }
    }
    Ok(())
}

fn ftcommon_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(FTCOMMON_GENERATED)
}

/// Translate the selected pinned decomp movement callbacks into the reusable
/// `game-ftcommon` boundary. Only the pure decision is translated; engine calls
/// are retained in the generated provenance.
fn ftcommon_port(root: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let revision = revision(root)?;
    let turn = std::fs::read_to_string(root.join(TURN_PATH))?;
    let jump = std::fs::read_to_string(root.join(JUMP_PATH))?;
    let air_jump = std::fs::read_to_string(root.join(AIR_JUMP_PATH))?;
    let functions = vec![
        lower_guard(&turn, TURN_PATH, "ftCo_800C97A8")?,
        lower_callback(&jump, JUMP_PATH, "ftCo_Jump_Enter")?,
        lower_callback(&air_jump, AIR_JUMP_PATH, "ftCo_JumpAerial_Enter_Basic")?,
    ];
    Ok(emit_port_rust(&PortFile {
        repository: MELEE_REPOSITORY,
        revision: &revision,
        functions: &functions,
    })?)
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

#[derive(Serialize)]
struct OperationRecord {
    symbol: &'static str,
    operation: &'static str,
    owner: &'static str,
}

#[derive(Serialize)]
struct InventoryCounts {
    files: usize,
    functions: usize,
    calls: usize,
    recognized: usize,
    unsupported: usize,
}

#[derive(Serialize)]
struct CommonInventory {
    repository: &'static str,
    revision: String,
    scope: &'static str,
    operations: Vec<OperationRecord>,
    files: Vec<String>,
    parse_errors: Vec<usize>,
    symbols: Vec<String>,
    functions: Vec<(usize, usize, String)>,
    calls: Vec<(usize, usize, usize, usize, Option<usize>)>,
    counts: InventoryCounts,
}

fn ftcommon_sources(root: &Path) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let mut names: Vec<String> = std::fs::read_dir(root.join(FTCOMMON_PATH))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<std::path::PathBuf>, std::io::Error>>()?
        .into_iter()
        .filter(|path| path.extension().is_some_and(|extension| extension == "c"))
        .filter_map(|path| path.file_name().map(|name| name.to_string_lossy().into_owned()))
        .collect();
    names.sort();
    let sources = names
        .into_iter()
        .map(|name| {
            let relative = format!("{FTCOMMON_PATH}/{name}");
            std::fs::read_to_string(root.join(&relative)).map(|source| (relative, source))
        })
        .collect::<Result<Vec<(String, String)>, std::io::Error>>()?;
    Ok(sources)
}

fn common_inventory_record(
    root: &Path,
) -> Result<(CommonInventory, Inventory), Box<dyn std::error::Error>> {
    let revision = revision(root)?;
    let inventory = common_inventory(&ftcommon_sources(root)?)?;
    let mut symbols: Vec<String> = inventory.calls.iter().map(|call| call.symbol.clone()).collect();
    symbols.sort();
    symbols.dedup();
    let symbol_index: BTreeMap<String, usize> = symbols
        .iter()
        .enumerate()
        .map(|(index, symbol)| (symbol.clone(), index))
        .collect();
    let calls = inventory
        .calls
        .iter()
        .map(|call| {
            (
                call.file,
                call.line,
                call.function,
                symbol_index[call.symbol.as_str()],
                call.operation,
            )
        })
        .collect();
    let functions = inventory
        .functions
        .iter()
        .map(|function| (function.file, function.line, function.name.clone()))
        .collect();
    let record = CommonInventory {
        repository: MELEE_REPOSITORY,
        revision,
        scope: FTCOMMON_PATH,
        operations: RECOGNIZED_OPERATIONS
            .iter()
            .map(|(symbol, operation, owner)| OperationRecord { symbol, operation, owner })
            .collect(),
        files: inventory.files.clone(),
        parse_errors: inventory.parse_errors.clone(),
        symbols,
        functions,
        calls,
        counts: InventoryCounts {
            files: inventory.files.len(),
            functions: inventory.function_count(),
            calls: inventory.call_count(),
            recognized: inventory.recognized_count(),
            unsupported: inventory.unsupported_count(),
        },
    };
    Ok((record, inventory))
}

fn operation_family(symbol: &str) -> &str {
    symbol.split_once('_').map_or(symbol, |(head, _)| head)
}

fn common_inventory_chart(record: &CommonInventory) -> String {
    let mut output = String::from(
        "# Generated by smash-import from pinned source syntax trees.\n\ndirection: right\n",
    );
    output.push_str(&format!(
        "scope: \"{} ({} files, {} functions, {} direct calls)\"\n",
        record.scope, record.counts.files, record.counts.functions, record.counts.calls,
    ));
    output.push_str(&format!(
        "recognized: \"recognized {}\"\nunsupported: \"unsupported {}\"\n",
        record.counts.recognized, record.counts.unsupported,
    ));
    output.push_str("scope -> recognized\nscope -> unsupported\n");

    let mut recognized: BTreeMap<usize, usize> = BTreeMap::new();
    for call in &record.calls {
        if let Some(operation) = call.4 {
            *recognized.entry(operation).or_default() += 1;
        }
    }
    for (operation, count) in &recognized {
        let entry = &record.operations[*operation];
        output.push_str(&format!(
            "r_{operation}: \"{}: {} ({count})\"\nrecognized -> r_{operation}\n",
            entry.operation, entry.symbol,
        ));
    }

    let mut families: BTreeMap<&str, (usize, std::collections::BTreeSet<&str>)> = BTreeMap::new();
    for call in &record.calls {
        if call.4.is_none() {
            let symbol = record.symbols[call.3].as_str();
            let family = families.entry(operation_family(symbol)).or_default();
            family.0 += 1;
            family.1.insert(symbol);
        }
    }
    let mut ranked: Vec<(&str, usize, usize)> = families
        .iter()
        .map(|(family, (calls, symbols))| (*family, *calls, symbols.len()))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let shown = ranked.iter().take(20);
    let mut tail = (0usize, 0usize);
    for (_, calls, symbols) in ranked.iter().skip(20) {
        tail = (tail.0 + calls, tail.1 + symbols);
    }
    for (family, calls, symbols) in shown {
        output.push_str(&format!(
            "u_{family}: \"{family} ({symbols} symbols, {calls} calls)\"\nunsupported -> u_{family}\n",
        ));
    }
    if tail.0 > 0 {
        output.push_str(&format!(
            "u_other: \"other ({} symbols, {} calls)\"\nunsupported -> u_other\n",
            tail.1, tail.0,
        ));
    }
    output
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
    }

    #[test]
    fn generated_common_inventory_is_current() {
        let (common, _) = super::common_inventory_record(&super::melee_root()).unwrap();
        assert_eq!(
            format!("{}\n", serde_json::to_string(&common).unwrap()),
            include_str!("fighters/falcon/generated/4_common_inventory.json"),
        );
        assert_eq!(
            super::common_inventory_chart(&common),
            include_str!("fighters/falcon/generated/4_common_inventory.d2"),
        );
    }

    #[test]
    fn common_inventory_covers_scope_with_sourced_calls() {
        let (common, inventory) = super::common_inventory_record(&super::melee_root()).unwrap();
        assert_eq!(common.counts.files, 142);
        assert_eq!(common.counts.functions, 1697);
        assert_eq!(common.counts.calls, 6614);
        assert_eq!(common.counts.recognized, 372);
        assert_eq!(common.counts.unsupported, 6242);
        assert_eq!(common.parse_errors.len(), 3);
        for call in &inventory.calls {
            assert!(call.file < inventory.files.len());
            assert!(call.function < inventory.functions.len());
            assert!(call.line >= 1);
            assert!(!call.symbol.is_empty());
            if let Some(operation) = call.operation {
                assert_eq!(game_content::RECOGNIZED_OPERATIONS[operation].0, call.symbol);
            }
        }
    }

    #[test]
    fn generated_ftcommon_port_is_current() {
        let generated = super::ftcommon_port(&super::melee_root()).unwrap();
        let path = super::ftcommon_path();
        assert_eq!(generated, std::fs::read_to_string(&path).unwrap());
        assert!(generated.contains("pub fn ftCo_800C97A8("));
        assert!(generated.contains(
            "pub fn ftCo_Jump_Enter(query: &FighterQuery, common: &CommonData) -> [FtCommonEffect; 4]",
        ));
        assert!(generated.contains(
            "pub fn ftCo_JumpAerial_Enter_Basic(query: &FighterQuery, common: &CommonData, attrs: &CoAttrs) -> [FtCommonEffect; 3]",
        ));
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
