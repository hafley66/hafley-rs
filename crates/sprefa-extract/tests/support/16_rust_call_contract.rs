use serde_json::Value;
#[path = "0_rust_names_call_contract.rs"]
mod contract;

pub fn capture(step: &Value) -> Value {
    let facts = std::fs::read_to_string(step["path"].as_str().unwrap()).unwrap()
        .lines().map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|row| {
            let (path, start) = if row["record"] == "resolved_edge" {
                (&row["caller_path"], &row["caller_site_start"])
            } else { (&row["path"], &row["span"]["start"]) };
            step["file"].as_str().is_none_or(|file| path.as_str().is_some_and(|path| path.ends_with(file)))
                && step["site"].as_u64().is_none_or(|site| start.as_u64() == Some(site))
        }).collect::<Vec<_>>();
    serde_json::json!(contract::call_contract(&facts))
}
