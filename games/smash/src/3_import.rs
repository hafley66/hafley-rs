use game_content::{
    Guard, Inventory, Op, PortFile, RECOGNIZED_OPERATIONS, SourceRef, SourceRule, TransitionSpec,
    Trigger, Unresolved, common_inventory, conditional_choice, decode_file, emit_chart,
    emit_port_rust, function_evidence, if_guard, lower_callback, lower_guard,
    source_machine_inventory,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const MELEE_REPOSITORY: &str = "https://github.com/doldecomp/melee.git";
const TURN_PATH: &str = "src/melee/ft/kinds/ftCommon/ftCo_Turn.c";
const JUMP_PATH: &str = "src/melee/ft/kinds/ftCommon/ftCo_Jump.c";
const AIR_JUMP_PATH: &str = "src/melee/ft/kinds/ftCommon/ftCo_JumpAerial.c";
const FTCOMMON_PATH: &str = "src/melee/ft/kinds/ftCommon";
const FTCOMMON_FORWARD_PATH: &str = "src/melee/ft/kinds/ftCommon/forward.h";
const FTCOMMON_GENERATED: &str = "../crates/ftcommon/src/generated/0_ftcommon.rs";
const SOURCE_INVENTORY_GENERATED: &str = "../classification/12_source_inventory.json";

#[derive(Serialize)]
struct SourceImport {
    repository: &'static str,
    revision: String,
    rules: Vec<SourceRule>,
    unresolved: Vec<Unresolved>,
}

fn pigeon_source() -> Result<String, Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/fighters/pigeon/imported");
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

fn pigeon(output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::write(output, pigeon_source()?)?;
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

fn pigeon_check(output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let source = source_import()?;
    let (common, _) = common_inventory_record(&melee_root())?;
    let expected = [
        (output.to_path_buf(), pigeon_source()?),
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
        return Err("cannot read pinned source submodule revision".into());
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

fn source_inventory_record(root: &Path) -> Result<game_content::SourceMachineInventory, Box<dyn std::error::Error>> {
    let revision = revision(root)?;
    let vocabulary = std::fs::read_to_string(root.join(FTCOMMON_FORWARD_PATH))?;
    Ok(source_machine_inventory(
        "melee-ftcommon",
        MELEE_REPOSITORY,
        &revision,
        FTCOMMON_FORWARD_PATH,
        &vocabulary,
        &ftcommon_sources(root)?,
    )?)
}

fn source_inventory(check: bool) -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SOURCE_INVENTORY_GENERATED);
    emit(&path, &source_inventory_record(&melee_root())?, check)
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

/// Uppercase constant identifier for one retained Rukaidata attribute name.
fn attribute_ident(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let ident = name.replace(' ', "_").to_uppercase();
    if !ident.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') {
        return Err(format!("invalid attribute identifier {ident}").into());
    }
    Ok(ident)
}

/// Render the numeric rows of one retained Rukaidata attributes page as Rust
/// constants. The caller supplies the page; this reusable derivation embeds no
/// runtime or display character name.
fn attribute_source(html: &str) -> Result<String, Box<dyn std::error::Error>> {
    let values = game_content::attributes(html)?;
    let mut output = String::from("// Generated by smash-import from retained PM3.6 attributes.html.\n#![allow(dead_code)]\n\n");
    for (name, value) in values {
        output.push_str(&format!("pub const {}: f32 = {value:?};\n", attribute_ident(&name)?));
    }
    Ok(output)
}

/// Explicit local locomotion policy for the `game_fighter::Rules` values absent
/// from retained PM3.6 attributes. These numbers are local choices with no
/// retained attribute and no claimed source provenance; this cut applies the
/// current Pigeon values to both characters.
#[derive(Clone, Copy, Debug)]
pub struct RulePolicy {
    pub walk_stick_threshold: f32,
    pub dash_stick_threshold: f32,
    pub dash_ticks: u32,
    pub dash_friction_mul: f32,
    pub crouch_enter_ticks: u32,
    pub crouch_exit_ticks: u32,
}

/// Local locomotion policy applied to both characters in this cut. The values
/// were carried from the current Pigeon local policy; they are unsourced, as no
/// retained PM3.6 attribute supplies them and no source provenance is claimed.
pub const BASE_POLICY: RulePolicy = RulePolicy {
    walk_stick_threshold: 0.2,
    dash_stick_threshold: 0.8,
    dash_ticks: 15,
    dash_friction_mul: 1.0,
    crouch_enter_ticks: 1,
    crouch_exit_ticks: 1,
};

/// One policy field, so emission stays named rather than positional.
#[derive(Clone, Copy)]
enum PolicyField {
    WalkStickThreshold,
    DashStickThreshold,
    DashTicks,
    DashFrictionMul,
    CrouchEnterTicks,
    CrouchExitTicks,
}

/// Where one `game_fighter::Rules` field is filled from.
#[derive(Clone, Copy)]
enum RuleSource {
    /// Float from the generated attribute constant.
    Attr(&'static str),
    /// Integral attribute emitted as the constant cast to `u32`.
    AttrU32(&'static str),
    /// Integral attribute emitted as the constant cast to `u8`.
    AttrU8(&'static str),
    /// Explicit local policy value.
    Policy(PolicyField),
}

impl RuleSource {
    /// The retained attribute name this field reads, if any.
    fn attribute(self) -> Option<&'static str> {
        match self {
            RuleSource::Attr(name) | RuleSource::AttrU32(name) | RuleSource::AttrU8(name) => Some(name),
            RuleSource::Policy(_) => None,
        }
    }
}

/// One `game_fighter::Rules` field and where it is filled from.
#[derive(Clone, Copy)]
struct RuleField {
    /// The `game_fighter::Rules` field name.
    field: &'static str,
    /// The origin of that field's value.
    source: RuleSource,
}

/// Every `game_fighter::Rules` field, in the authored runtime order, mapped to
/// its origin. Attribute names are the retained `attributes.html` vocabulary.
const RULES: [RuleField; 33] = [
    RuleField { field: "walk_init_vel", source: RuleSource::Attr("walk init vel") },
    RuleField { field: "walk_accel", source: RuleSource::Attr("walk acc") },
    RuleField { field: "walk_max_vel", source: RuleSource::Attr("walk max vel") },
    RuleField { field: "walk_stick_threshold", source: RuleSource::Policy(PolicyField::WalkStickThreshold) },
    RuleField { field: "dash_stick_threshold", source: RuleSource::Policy(PolicyField::DashStickThreshold) },
    RuleField { field: "dash_initial_velocity", source: RuleSource::Attr("dash init vel") },
    RuleField { field: "dash_accel_base", source: RuleSource::Attr("dash run acc b") },
    RuleField { field: "dash_accel_mul", source: RuleSource::Attr("dash run acc a") },
    RuleField { field: "dash_max_velocity", source: RuleSource::Attr("dash run term vel") },
    RuleField { field: "dash_ticks", source: RuleSource::Policy(PolicyField::DashTicks) },
    RuleField { field: "ground_friction", source: RuleSource::Attr("ground friction") },
    RuleField { field: "dash_friction_mul", source: RuleSource::Policy(PolicyField::DashFrictionMul) },
    RuleField { field: "ground_max_horizontal_velocity", source: RuleSource::Attr("grounded max x vel") },
    RuleField { field: "turn_ticks", source: RuleSource::AttrU32("flip dir frame") },
    RuleField { field: "jump_startup_time", source: RuleSource::AttrU32("jump squat frames") },
    RuleField { field: "crouch_enter_ticks", source: RuleSource::Policy(PolicyField::CrouchEnterTicks) },
    RuleField { field: "crouch_exit_ticks", source: RuleSource::Policy(PolicyField::CrouchExitTicks) },
    RuleField { field: "jump_h_initial_velocity", source: RuleSource::Attr("jump x init vel") },
    RuleField { field: "jump_h_max_velocity", source: RuleSource::Attr("jump x init term vel") },
    RuleField { field: "jump_v_initial_velocity", source: RuleSource::Attr("jump y init vel") },
    RuleField { field: "hop_v_initial_velocity", source: RuleSource::Attr("jump y init vel short") },
    RuleField { field: "ground_to_air_jump_momentum_multiplier", source: RuleSource::Attr("jump x vel ground mult") },
    RuleField { field: "max_jumps", source: RuleSource::AttrU8("num jumps") },
    RuleField { field: "air_jump_v_multiplier", source: RuleSource::Attr("air jump y mult") },
    RuleField { field: "air_jump_h_multiplier", source: RuleSource::Attr("air jump x mult") },
    RuleField { field: "gravity", source: RuleSource::Attr("gravity") },
    RuleField { field: "terminal_velocity", source: RuleSource::Attr("term vel") },
    RuleField { field: "fast_fall_velocity", source: RuleSource::Attr("fastfall velocity") },
    RuleField { field: "air_drift_stick_mul", source: RuleSource::Attr("air mobility a") },
    RuleField { field: "air_drift_base", source: RuleSource::Attr("air mobility b") },
    RuleField { field: "air_drift_max", source: RuleSource::Attr("air x term vel") },
    RuleField { field: "aerial_friction", source: RuleSource::Attr("air friction x") },
    RuleField { field: "landing_lag", source: RuleSource::AttrU32("normal landing lag") },
];

/// Reject a non-finite value before it reaches generated output.
fn finite(value: f32, name: &str) -> Result<f32, Box<dyn std::error::Error>> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("non-finite attribute {name}: {value:?}").into())
    }
}

/// Exact non-negative integer conversion for a `u32` field.
fn exact_u32(value: f32, name: &str) -> Result<u32, Box<dyn std::error::Error>> {
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value >= 4_294_967_296.0 {
        return Err(format!("invalid u32 conversion for {name}: {value:?}").into());
    }
    Ok(value as u32)
}

/// Exact non-negative integer conversion for a `u8` field.
fn exact_u8(value: f32, name: &str) -> Result<u8, Box<dyn std::error::Error>> {
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > u8::MAX as f32 {
        return Err(format!("invalid u8 conversion for {name}: {value:?}").into());
    }
    Ok(value as u8)
}

/// Render one policy value as an explicit labeled literal.
fn policy_token(policy: &RulePolicy, field: PolicyField) -> Result<String, Box<dyn std::error::Error>> {
    match field {
        PolicyField::WalkStickThreshold => {
            Ok(format!("{:?}", finite(policy.walk_stick_threshold, "walk_stick_threshold")?))
        }
        PolicyField::DashStickThreshold => {
            Ok(format!("{:?}", finite(policy.dash_stick_threshold, "dash_stick_threshold")?))
        }
        PolicyField::DashTicks => Ok(policy.dash_ticks.to_string()),
        PolicyField::DashFrictionMul => {
            Ok(format!("{:?}", finite(policy.dash_friction_mul, "dash_friction_mul")?))
        }
        PolicyField::CrouchEnterTicks => Ok(policy.crouch_enter_ticks.to_string()),
        PolicyField::CrouchExitTicks => Ok(policy.crouch_exit_ticks.to_string()),
    }
}

/// Render the complete, source-free `game_fighter::Rules` constructor for one
/// character. Attribute fields reference the sibling generated vocabulary;
/// policy fields are emitted as explicit labeled literals. A missing or
/// duplicate attribute mapping, non-finite value, or invalid integer
/// conversion fails generation. No character name is embedded.
fn rules_source(
    attributes: &BTreeMap<String, f32>,
    policy: &RulePolicy,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut seen = std::collections::BTreeSet::new();
    for RuleField { field, source } in RULES {
        let Some(name) = source.attribute() else { continue };
        if !seen.insert(name) {
            return Err(format!("duplicate attribute mapping for {name}").into());
        }
        let value = attributes
            .get(name)
            .copied()
            .ok_or_else(|| format!("missing attribute {name} for field {field}"))?;
        finite(value, name)?;
        match source {
            RuleSource::AttrU32(_) => { exact_u32(value, name)?; }
            RuleSource::AttrU8(_) => { exact_u8(value, name)?; }
            _ => {}
        }
    }

    let mut output = String::from(concat!(
        "// Generated by smash-import from retained PM3.6 attributes.html.\n",
        "// Attribute fields use the generated attribute vocabulary; policy fields\n",
        "// are explicit local choices carried from current Pigeon policy, unsourced.\n",
        "#![allow(dead_code)]\n",
        "\n",
        "pub fn rules() -> game_fighter::Rules {\n",
        "    game_fighter::Rules {\n",
    ));
    for RuleField { field, source } in RULES {
        let (token, label) = match source {
            RuleSource::Attr(name) => (format!("super::attr::{}", attribute_ident(name)?), ""),
            RuleSource::AttrU32(name) => (format!("super::attr::{} as u32", attribute_ident(name)?), ""),
            RuleSource::AttrU8(name) => (format!("super::attr::{} as u8", attribute_ident(name)?), ""),
            RuleSource::Policy(field) => (policy_token(policy, field)?, " // policy"),
        };
        output.push_str(&format!("        {field}: {token},{label}\n"));
    }
    output.push_str("    }\n}\n");
    Ok(output)
}

/// Parsed CLI invocation. The only accepted first spellings are `pigeon`,
/// `catalog`, and `attributes`; `pigeon` additionally accepts one optional
/// output path.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Command {
    Pigeon { check: bool, output: Option<PathBuf> },
    Catalog { check: bool },
    Attributes { check: bool },
    SourceInventory { check: bool },
}

const USAGE: &str = "usage: smash-import <pigeon|catalog|attributes|source-inventory> [--check] [output]";
const PIGEON_USAGE: &str = "usage: smash-import pigeon [--check] [output]";
const CATALOG_USAGE: &str = "usage: smash-import catalog [--check]";
const ATTRIBUTES_USAGE: &str = "usage: smash-import attributes [--check]";
const SOURCE_INVENTORY_USAGE: &str = "usage: smash-import source-inventory [--check]";

/// Pure argument parser. `catalog` and `attributes` accept exactly no argument
/// (generate) or one `--check` (verify); `pigeon` accepts an optional
/// `--check` and output path. Any unknown first argument or extra argument
/// returns the matching usage error.
fn parse_args(args: &[std::ffi::OsString]) -> Result<Command, String> {
    let is_check = |arg: &std::ffi::OsString| arg == std::ffi::OsStr::new("--check");
    let mut args = args.iter();
    match args.next().map(std::ffi::OsString::as_os_str) {
        Some(command) if command == std::ffi::OsStr::new("pigeon") => {
            let second = args.next();
            let check = second.is_some_and(is_check);
            let output = if check { args.next() } else { second };
            if args.next().is_some() {
                return Err(PIGEON_USAGE.into());
            }
            Ok(Command::Pigeon { check, output: output.map(PathBuf::from) })
        }
        Some(command) if command == std::ffi::OsStr::new("catalog") => {
            let second = args.next();
            let check = second.is_some_and(is_check);
            if second.is_some() && !check {
                return Err(CATALOG_USAGE.into());
            }
            if args.next().is_some() {
                return Err(CATALOG_USAGE.into());
            }
            Ok(Command::Catalog { check })
        }
        Some(command) if command == std::ffi::OsStr::new("attributes") => {
            let second = args.next();
            let check = second.is_some_and(is_check);
            if second.is_some() && !check {
                return Err(ATTRIBUTES_USAGE.into());
            }
            if args.next().is_some() {
                return Err(ATTRIBUTES_USAGE.into());
            }
            Ok(Command::Attributes { check })
        }
        Some(command) if command == std::ffi::OsStr::new("source-inventory") => {
            let second = args.next();
            let check = second.is_some_and(is_check);
            if second.is_some() && !check {
                return Err(SOURCE_INVENTORY_USAGE.into());
            }
            if args.next().is_some() {
                return Err(SOURCE_INVENTORY_USAGE.into());
            }
            Ok(Command::SourceInventory { check })
        }
        _ => Err(USAGE.into()),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    match parse_args(&args).map_err(|usage| -> Box<dyn std::error::Error> { usage.into() })? {
        Command::Pigeon { check, output } => {
            let output = output.unwrap_or_else(|| {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("src/fighters/pigeon/generated/0_chart.rs")
            });
            if check { pigeon_check(&output) } else { pigeon(&output) }
        }
        Command::Catalog { check } => character_catalogs(check),
        Command::Attributes { check } => character_attributes(check),
        Command::SourceInventory { check } => source_inventory(check),
    }
}

/// Write or verify one committed character output. Serialization is always
/// pretty JSON with a trailing newline, matching the committed files.
fn emit(path: &Path, value: &impl Serialize, check: bool) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = format!("{}\n", serde_json::to_string_pretty(value)?);
    if check {
        if std::fs::read_to_string(path)? != bytes {
            return Err(format!("stale generated output: {}", path.display()).into());
        }
    } else {
        std::fs::write(path, bytes)?;
    }
    Ok(())
}

/// The one direct generate/check path covering both characters' attribute and
/// rules modules. Each character contributes its own retained `attributes.html`;
/// the reusable [`attribute_source`] and [`rules_source`] derivations embed no
/// character name. Both characters share the explicit [`BASE_POLICY`] in this
/// cut.
fn character_attributes(check: bool) -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fighters");
    let pigeon = include_str!("fighters/pigeon/imported/attributes.html");
    emit_text(&root.join("pigeon/generated/1_attributes.rs"), &attribute_source(pigeon)?, check)?;
    emit_text(
        &root.join("pigeon/generated/7_rules.rs"),
        &rules_source(&game_content::attributes(pigeon)?, &BASE_POLICY)?,
        check,
    )?;
    let dog = include_str!("fighters/dog/imported/attributes.html");
    emit_text(&root.join("dog/generated/2_attributes.rs"), &attribute_source(dog)?, check)?;
    emit_text(
        &root.join("dog/generated/3_rules.rs"),
        &rules_source(&game_content::attributes(dog)?, &BASE_POLICY)?,
        check,
    )?;
    Ok(())
}

/// Write or verify one committed text output, comparing exact bytes.
fn emit_text(path: &Path, contents: &str, check: bool) -> Result<(), Box<dyn std::error::Error>> {
    if check {
        if std::fs::read_to_string(path)? != contents {
            return Err(format!("stale generated output: {}", path.display()).into());
        }
    } else {
        std::fs::write(path, contents)?;
    }
    Ok(())
}

/// The one direct generate/check path covering both characters. Derivation goes
/// through each fighter's own [`Spec`] and shared `game_content::generate_catalog`;
/// this CLI never re-derives a catalog or reparses Rust. Role bindings derive
/// from the same evidence and are emitted as named, source-free Rust plus JSON.
fn character_catalogs(check: bool) -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fighters");
    let pigeon = smash::fighters::pigeon::catalog::generate()?;
    emit(&root.join("pigeon/generated/5_catalog.json"), &pigeon.evidence, check)?;
    emit(&root.join("pigeon/generated/6_baked.json"), &pigeon.actions, check)?;
    emit_roles(
        &root.join("pigeon/generated"),
        "8_roles.json",
        "8_roles.rs",
        &smash::fighters::pigeon::catalog::generate_roles(&pigeon.evidence)?,
        check,
    )?;
    let dog = smash::fighters::dog::catalog::generate()?;
    emit(&root.join("dog/generated/0_catalog.json"), &dog.evidence, check)?;
    emit(&root.join("dog/generated/1_baked.json"), &dog.actions, check)?;
    emit_roles(
        &root.join("dog/generated"),
        "4_roles.json",
        "4_roles.rs",
        &smash::fighters::dog::catalog::generate_roles(&dog.evidence)?,
        check,
    )?;
    Ok(())
}

/// Emit one character's generated role bindings: JSON evidence plus the named,
/// source-free Rust table. Each character owns its artifact slot.
fn emit_roles(
    generated: &Path,
    json: &str,
    source: &str,
    roles: &game_content::RoleBindings,
    check: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    emit(&generated.join(json), roles, check)?;
    emit_text(&generated.join(source), &game_content::role_bindings_source(roles), check)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn generated_pigeon_chart_is_current() {
        assert_eq!(
            super::pigeon_source().unwrap(),
            include_str!("fighters/pigeon/generated/0_chart.rs"),
        );
    }

    #[test]
    fn generated_attributes_are_current() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fighters");
        let pigeon = super::attribute_source(include_str!("fighters/pigeon/imported/attributes.html")).unwrap();
        assert_eq!(
            pigeon,
            std::fs::read_to_string(root.join("pigeon/generated/1_attributes.rs")).unwrap(),
        );
        let dog = super::attribute_source(include_str!("fighters/dog/imported/attributes.html")).unwrap();
        assert_eq!(
            dog,
            std::fs::read_to_string(root.join("dog/generated/2_attributes.rs")).unwrap(),
        );
    }

    #[test]
    fn generated_role_bindings_are_current() {
        let pigeon = smash::fighters::pigeon::catalog::generate().unwrap();
        let roles = smash::fighters::pigeon::catalog::generate_roles(&pigeon.evidence).unwrap();
        assert_eq!(
            format!("{}\n", serde_json::to_string_pretty(&roles).unwrap()),
            include_str!("fighters/pigeon/generated/8_roles.json"),
        );
        assert_eq!(
            game_content::role_bindings_source(&roles),
            include_str!("fighters/pigeon/generated/8_roles.rs"),
        );
        assert_eq!(roles.missing(), Vec::new(), "Pigeon binds every role");

        let dog = smash::fighters::dog::catalog::generate().unwrap();
        let roles = smash::fighters::dog::catalog::generate_roles(&dog.evidence).unwrap();
        assert_eq!(
            format!("{}\n", serde_json::to_string_pretty(&roles).unwrap()),
            include_str!("fighters/dog/generated/4_roles.json"),
        );
        assert_eq!(
            game_content::role_bindings_source(&roles),
            include_str!("fighters/dog/generated/4_roles.rs"),
        );
        assert_eq!(roles.missing(), Vec::new(), "Dog binds every role");
        assert_eq!(roles.unavailable(), Vec::new(), "Dog leaves no role unavailable");
        assert!(
            roles.roles.iter().all(|binding| binding.fallback.is_none()),
            "Dog declares no fallback",
        );
    }

    #[test]
    fn generated_rules_are_current() {
        let pigeon = game_content::attributes(include_str!("fighters/pigeon/imported/attributes.html")).unwrap();
        assert_eq!(
            super::rules_source(&pigeon, &super::BASE_POLICY).unwrap(),
            include_str!("fighters/pigeon/generated/7_rules.rs"),
        );
        let dog = game_content::attributes(include_str!("fighters/dog/imported/attributes.html")).unwrap();
        assert_eq!(
            super::rules_source(&dog, &super::BASE_POLICY).unwrap(),
            include_str!("fighters/dog/generated/3_rules.rs"),
        );
    }

    #[test]
    fn rules_source_rejects_missing_nonfinite_and_invalid_integer() {
        let good = game_content::attributes(include_str!("fighters/pigeon/imported/attributes.html")).unwrap();
        assert!(super::rules_source(&good, &super::BASE_POLICY).is_ok());
        let mut missing = good.clone();
        missing.remove("walk init vel");
        assert!(super::rules_source(&missing, &super::BASE_POLICY).is_err());
        let mut fractional = good.clone();
        fractional.insert("num jumps".into(), 2.5);
        assert!(super::rules_source(&fractional, &super::BASE_POLICY).is_err());
        let mut nonfinite = good.clone();
        nonfinite.insert("flip dir frame".into(), f32::INFINITY);
        assert!(super::rules_source(&nonfinite, &super::BASE_POLICY).is_err());
        let mut overflow = good.clone();
        overflow.insert("num jumps".into(), 300.0);
        assert!(super::rules_source(&overflow, &super::BASE_POLICY).is_err());
    }

    #[test]
    fn generated_source_rules_are_current() {
        let import = super::source_import().unwrap();
        let json = format!("{}\n", serde_json::to_string_pretty(&import).unwrap());
        assert_eq!(json, include_str!("fighters/pigeon/generated/2_source_rules.json"));
        assert_eq!(
            super::source_chart(&import),
            include_str!("fighters/pigeon/generated/3_source_chart.d2"),
        );
    }

    #[test]
    fn generated_common_inventory_is_current() {
        let (common, _) = super::common_inventory_record(&super::melee_root()).unwrap();
        assert_eq!(
            format!("{}\n", serde_json::to_string(&common).unwrap()),
            include_str!("fighters/pigeon/generated/4_common_inventory.json"),
        );
        assert_eq!(
            super::common_inventory_chart(&common),
            include_str!("fighters/pigeon/generated/4_common_inventory.d2"),
        );
    }

    #[test]
    fn generated_source_inventory_is_current_and_pinned() {
        let record = super::source_inventory_record(&super::melee_root()).unwrap();
        assert_eq!(record.revision, super::revision(&super::melee_root()).unwrap());
        assert_eq!(record.counts.states, record.states.len());
        assert_eq!(record.counts.callbacks, record.callbacks.len());
        assert_eq!(
            record.counts.direct_calls,
            record.callbacks.iter().map(|callback| callback.calls.len()).sum::<usize>(),
        );
        assert_eq!(record.counts.unresolved, record.unresolved.len());
        assert!(record.states.windows(2).all(|states| states[0].ordinal < states[1].ordinal));
        assert!(record.callbacks.iter().flat_map(|callback| &callback.calls).any(|call| {
            call.operation.is_none()
        }));
        assert_eq!(
            format!("{}\n", serde_json::to_string_pretty(&record).unwrap()),
            include_str!("../../classification/12_source_inventory.json"),
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

    /// Only `pigeon`, `catalog`, and `attributes` are accepted spellings.
    /// Catalog and attributes accept only generate or `--check`.
    #[test]
    fn catalog_cli_spellings_are_exact() {
        use super::{
            ATTRIBUTES_USAGE, CATALOG_USAGE, Command, PIGEON_USAGE, SOURCE_INVENTORY_USAGE,
            USAGE, parse_args,
        };
        use std::ffi::OsString;

        let args = |parts: &[&str]| parts.iter().map(OsString::from).collect::<Vec<_>>();
        assert_eq!(
            parse_args(&args(&["pigeon"])).unwrap(),
            Command::Pigeon { check: false, output: None },
        );
        assert_eq!(
            parse_args(&args(&["pigeon", "--check"])).unwrap(),
            Command::Pigeon { check: true, output: None },
        );
        assert_eq!(
            parse_args(&args(&["pigeon", "out.rs"])).unwrap(),
            Command::Pigeon { check: false, output: Some("out.rs".into()) },
        );
        assert_eq!(parse_args(&args(&["catalog"])).unwrap(), Command::Catalog { check: false });
        assert_eq!(
            parse_args(&args(&["catalog", "--check"])).unwrap(),
            Command::Catalog { check: true },
        );
        assert_eq!(
            parse_args(&args(&["attributes"])).unwrap(),
            Command::Attributes { check: false },
        );
        assert_eq!(
            parse_args(&args(&["attributes", "--check"])).unwrap(),
            Command::Attributes { check: true },
        );
        assert_eq!(
            parse_args(&args(&["source-inventory"])).unwrap(),
            Command::SourceInventory { check: false },
        );
        assert_eq!(
            parse_args(&args(&["source-inventory", "--check"])).unwrap(),
            Command::SourceInventory { check: true },
        );
        for bad in [
            vec![],
            vec!["cat"],
            vec!["Pigeon"],
            vec!["Catalog"],
            vec!["Attributes"],
            vec!["--check"],
        ] {
            assert_eq!(parse_args(&args(&bad)), Err(USAGE.into()), "{bad:?}");
        }
        for bad in [
            vec!["source-inventory", "out.json"],
            vec!["source-inventory", "--check", "extra"],
        ] {
            assert_eq!(
                parse_args(&args(&bad)),
                Err(SOURCE_INVENTORY_USAGE.into()),
                "{bad:?}"
            );
        }
        for bad in [
            vec!["catalog", "out.rs"],
            vec!["catalog", "extra"],
            vec!["catalog", "generate"],
            vec!["catalog", "--check", "extra"],
            vec!["catalog", "extra", "--check"],
        ] {
            assert_eq!(parse_args(&args(&bad)), Err(CATALOG_USAGE.into()), "{bad:?}");
        }
        for bad in [
            vec!["attributes", "out.rs"],
            vec!["attributes", "--check", "extra"],
            vec!["attributes", "extra", "--check"],
        ] {
            assert_eq!(parse_args(&args(&bad)), Err(ATTRIBUTES_USAGE.into()), "{bad:?}");
        }
        for bad in [vec!["pigeon", "out.rs", "extra"], vec!["pigeon", "--check", "a", "b"]] {
            assert_eq!(parse_args(&args(&bad)), Err(PIGEON_USAGE.into()), "{bad:?}");
        }
    }
}
