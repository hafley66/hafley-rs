use clap::ArgAction;

pub const EXAMPLES: &[(&str, &[&str])] = &[
    ("capabilities", &["ryi capabilities", "ryi capabilities --format jsonl"]),
    ("schema", &["ryi schema", "ryi schema --format jsonl"]),
    ("fast", &["ryi fast crates/sprefa-extract/tests/fixtures/type_ladder/src/_1_none.rs", "ryi fast --lines crates/sprefa-extract/tests/fixtures/type_ladder/src/_1_none.rs"]),
    ("slow", &["ryi slow --no-checker --scip-index crates/sprefa-extract/tests/fixtures/scip_relationship/fixture.scip crates/sprefa-extract/tests/fixtures/scip_relationship/animal.ts", "ryi slow --no-checker --lines --scip-index crates/sprefa-extract/tests/fixtures/scip_relationship/fixture.scip crates/sprefa-extract/tests/fixtures/scip_relationship/animal.ts"]),
    ("scip", &["ryi scip --root crates/sprefa-extract/tests/fixtures/scip_relationship --scip-index crates/sprefa-extract/tests/fixtures/scip_relationship/fixture.scip --raw", "ryi scip --root crates/sprefa-extract/tests/fixtures/scip_relationship --scip-index crates/sprefa-extract/tests/fixtures/scip_relationship/fixture.scip --raw --records document"]),
    ("ingest", &["ryi ingest crates/sprefa-extract/tests/fixtures/ingest/00_foreign.jsonl", "ryi ingest crates/sprefa-extract/tests/fixtures/ingest/05_symbol.jsonl"]),
    ("query", &["ryi query --query '(function_item name: (identifier) @name)' crates/sprefa-extract/tests/fixtures/type_ladder/src/_1_none.rs", "ryi query --lang rust --query '(struct_item name: (type_identifier) @name)' crates/sprefa-extract/tests/fixtures/type_ladder/src/_1_none.rs"]),
    ("graph", &["ryi graph --callers slug --root crates/sprefa-extract/tests/fixtures/cleave_ts/basic crates/sprefa-extract/tests/fixtures/cleave_ts/basic", "ryi graph --from boot --root crates/sprefa-extract/tests/fixtures/cleave_ts/basic crates/sprefa-extract/tests/fixtures/cleave_ts/basic"]),
    ("trail", &["ryi trail 1", "ryi trail 3"]),
    ("diff", &["ryi diff --from HEAD --to HEAD --root . --pattern '*.ts'", "ryi diff --from HEAD --to HEAD --root . --pattern '*.rs'"]),
    ("watch", &["ryi watch --root crates/sprefa-extract/tests/fixtures/cleave_ts/basic --once", "ryi watch --root crates/sprefa-extract/tests/fixtures/cleave_ts/basic --once --pattern '*.ts'"]),
    ("stratify", &["ryi stratify --from crates/sprefa-extract/tests/fixtures/cleave_ts/basic/src/app.ts --root crates/sprefa-extract/tests/fixtures/cleave_ts/basic crates/sprefa-extract/tests/fixtures/cleave_ts/basic", "ryi stratify --kind call --from crates/sprefa-extract/tests/fixtures/cleave_ts/basic/src/app.ts --root crates/sprefa-extract/tests/fixtures/cleave_ts/basic crates/sprefa-extract/tests/fixtures/cleave_ts/basic"]),
    ("rename", &["ryi rename crates/sprefa-extract/tests/fixtures/cleave_ts/basic/src/util.ts#slug slugged --root crates/sprefa-extract/tests/fixtures/cleave_ts/basic", "ryi rename crates/sprefa-extract/tests/fixtures/cleave_ts/basic/src/util.ts#slug slugged --root crates/sprefa-extract/tests/fixtures/cleave_ts/basic --text-refs"]),
    ("move", &["ryi move crates/sprefa-extract/tests/fixtures/cleave_ts/basic/src/util.ts crates/sprefa-extract/tests/fixtures/cleave_ts/basic/src/utils.ts --root crates/sprefa-extract/tests/fixtures/cleave_ts/basic", "ryi move crates/sprefa-extract/tests/fixtures/cleave_ts/basic/src/util.ts crates/sprefa-extract/tests/fixtures/cleave_ts/basic/src/utils.ts --root crates/sprefa-extract/tests/fixtures/cleave_ts/basic --text-refs"]),
    ("cleave", &["ryi cleave crates/sprefa-extract/tests/fixtures/cleave_ts/basic/src/util.ts#slug crates/sprefa-extract/tests/fixtures/cleave_ts/basic/src/slug.ts --root crates/sprefa-extract/tests/fixtures/cleave_ts/basic", "ryi cleave crates/sprefa-extract/tests/fixtures/cleave_ts/basic/src/util.ts#slug crates/sprefa-extract/tests/fixtures/cleave_ts/basic/src/slug.ts --root crates/sprefa-extract/tests/fixtures/cleave_ts/basic --text-refs"]),
    ("region", &["ryi region crates/sprefa-extract/tests/fixtures/help_region/0_region.dl7 demo --generated crates/sprefa-extract/tests/fixtures/help_region/1_body.txt", "ryi region crates/sprefa-extract/tests/fixtures/help_region/0_region.dl7 demo --generated crates/sprefa-extract/tests/fixtures/help_region/1_body.txt --format jsonl"]),
];

pub fn compact_help(command: &clap::Command, examples: &[&str]) -> String {
    let name = command.get_name();
    let usage = if name == "graph" {
        "ryi graph <QUESTION> [PATH]...".to_owned()
    } else {
        command.clone().bin_name(if name == "ryi" { "ryi".to_owned() } else { format!("ryi {name}") }).render_usage().to_string().replace("Usage: ", "")
    };
    let mut text = format!("Usage: {usage}\n");
    if let Some(about) = command.get_about() { text.push_str(&format!("{about}\n")); }
    text.push_str("\nExamples:\n");
    for example in examples { text.push_str(&format!("  {example}\n")); }
    let common = ["root", "pattern", "entry", "depth", "lines"];
    let mut input_flags = Vec::new();
    let mut value_flags = Vec::new();
    let mut question_flags = Vec::new();
    let mut switches = Vec::new();
    for arg in command.get_arguments() {
        let id = arg.get_id().as_str();
        if matches!(id, "help" | "version" | "format" | "no_next" | "budget" | "human") || arg.is_global_set() || arg.get_long().is_none() { continue; }
        let spelling = flag_spelling(arg);
        if common.contains(&id) { input_flags.push(spelling); continue; }
        let doc = match id {
            "flow_path" => "value paths from PATH@START:END or tagged BLOB@START:END; UTF-8 offsets",
            "callers" => "incoming calls to [PATH#]NAME",
            "from" => "reachability from [PATH#]NAME",
            "call_path" => "shortest call paths from [PATH#]NAME",
            "type_path" => "shortest type paths from [PATH#]NAME",
            _ => "",
        };
        let doc = if doc.is_empty() { arg.get_help().map(ToString::to_string).unwrap_or_default() } else { doc.to_owned() };
        let doc = doc.split(';').next().unwrap_or("");
        let row = format!("{spelling}  {doc}");
        if name == "graph" && matches!(id, "callers" | "uses" | "from" | "call_path" | "type_path") { question_flags.push(row); }
        else if matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse) { switches.push(row); }
        else { value_flags.push(row); }
    }
    if !input_flags.is_empty() { text.push_str(&format!("\nCommon: {}\n", input_flags.join(" | "))); }
    for group in question_flags.chunks(3) { text.push_str(&format!("  {}\n", group.join(" | "))); }
    if !value_flags.is_empty() { text.push_str("\nFlags:\n"); for flag in value_flags { text.push_str(&format!("  {flag}\n")); } }
    if !switches.is_empty() { text.push_str("Switches:\n"); for group in switches.chunks(2) { text.push_str(&format!("  {}\n", group.join(" | "))); } }
    text
}

pub fn flag_spelling(arg: &clap::Arg) -> String {
    let name = format!("--{}", arg.get_long().unwrap());
    if matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse) { return name; }
    let value = arg.get_value_names().and_then(|names| names.first()).map(ToString::to_string).unwrap_or_else(|| arg.get_id().to_string().to_uppercase());
    format!("{name} <{value}>")
}
