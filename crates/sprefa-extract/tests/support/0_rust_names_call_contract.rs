//! Stable whole-call output for Names fixture tables.
use serde_json::Value;

pub fn call_contract(facts: &[Value]) -> String {
    let leaf = |path: &Value| {
        path.as_str()
            .unwrap_or("")
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string()
    };
    let mut rows = Vec::new();
    for fact in facts {
        if fact["record"] == "resolved_edge" {
            rows.push(format!(
                "edge {}:{} {} -> {}:{} {} {} {}",
                leaf(&fact["caller_path"]),
                fact["caller_site_start"],
                fact["caller_name"].as_str().unwrap_or("-"),
                leaf(&fact["callee_path"]),
                fact["callee_start"],
                fact["callee_name"].as_str().unwrap_or("-"),
                fact["kind"].as_str().unwrap_or(""),
                fact["resolution_origin"].as_str().unwrap_or("")
            ));
        } else if fact["record"] == "unresolved" && fact["family"] == "call" {
            rows.push(format!(
                "drop {}:{} {} {}",
                leaf(&fact["path"]),
                fact["span"]["start"],
                fact["detail"].as_str().unwrap_or(""),
                fact["reason"].as_str().unwrap_or("")
            ));
        }
    }
    rows.sort();
    rows.join("\n")
}
