use clap::ArgAction;

pub const EXAMPLES: &[(&str, &[&str])] = &[
    ("capabilities", &["ryi capabilities"]),
    ("schema", &["ryi schema"]),
    ("fast", &["ryi fast src/"]),
    ("slow", &["ryi slow --scip-index index.scip src/a.ts"]),
    ("scip", &["ryi scip --root . --raw"]),
    ("ingest", &["ryi ingest rows.jsonl"]),
    ("query", &["ryi query --query '(function_item name: (identifier) @n)' src/"]),
    ("graph", &["ryi graph --callers src/a.ts#slug --root . src/"]),
    ("trail", &["ryi trail 1"]),
    ("diff", &["ryi diff --from HEAD~1 --to HEAD --root ."]),
    ("watch", &["ryi watch --root . --once"]),
    ("stratify", &["ryi stratify --from src/main.ts --root . src/"]),
    ("rename", &["ryi rename src/a.ts#slug slugged --root ."]),
    ("move", &["ryi move src/a.ts src/b.ts --root ."]),
    ("cleave", &["ryi cleave src/a.ts#slug src/slug.ts --root ."]),
    ("region", &["ryi region gen.rs demo --generated body.txt"]),
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
    for example in examples { text.push_str(&format!("e.g. {example}\n")); }
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
            "callers" => "incoming calls",
            "uses" => "type uses",
            "from" => "reachable set",
            "call_path" => "shortest call paths",
            "type_path" => "shortest type paths",
            _ => "",
        };
        let doc = if doc.is_empty() { arg.get_help().map(ToString::to_string).unwrap_or_default() } else { doc.to_owned() };
        let doc = doc.split(';').next().unwrap_or("");
        let row = format!("{spelling}  {doc}");
        if name == "graph" && matches!(id, "callers" | "uses" | "from" | "call_path" | "type_path") { question_flags.push(row); }
        else if matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse) { switches.push(row); }
        else { value_flags.push(row); }
    }
    for row in question_flags { text.push_str(&format!("  {row}\n")); }
    if !input_flags.is_empty() { text.push_str(&format!("Input: {}\n", input_flags.join(" "))); }
    let bare = |rows: Vec<String>| rows.iter().map(|row| row.split("  ").next().unwrap().to_owned()).collect::<Vec<_>>().join(" ");
    if !value_flags.is_empty() { text.push_str(&format!("Flags: {}\n", bare(value_flags))); }
    if !switches.is_empty() { text.push_str(&format!("Switches: {}\n", bare(switches))); }
    text
}

pub fn flag_spelling(arg: &clap::Arg) -> String {
    let name = format!("--{}", arg.get_long().unwrap());
    if matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse) { return name; }
    let value = arg.get_value_names().and_then(|names| names.first()).map(ToString::to_string).unwrap_or_else(|| arg.get_id().to_string().to_uppercase());
    format!("{name} <{value}>")
}
