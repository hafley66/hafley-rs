use serde_json::Value;
use sprefa_extract::{content_id_of, FlatFact, tsi::Arg};
use std::collections::{BTreeMap, BTreeSet};

pub fn implementations(step: &Value) -> Value {
    let rows: Vec<FlatFact> = std::fs::read_to_string(step["path"].as_str().unwrap()).unwrap()
        .lines().filter(|line| !line.trim().is_empty()).map(|line| serde_json::from_str(line).unwrap()).collect();
    let facts: Vec<_> = rows.iter().filter_map(|row| match row { FlatFact::Fact(fact) => Some(fact), _ => None }).collect();
    let blobs: BTreeMap<_, _> = step["files"].as_array().unwrap().iter().map(|file| {
        let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(file.as_str().unwrap())).unwrap();
        (content_id_of(&bytes).to_string(), bytes)
    }).collect();
    let written = |arg: &Arg| {
        let Arg::Id(id) = arg else { panic!("an impl names its owner and trait"); };
        facts.iter().find(|fact| fact.relation == "tsi.origin" && matches!(fact.args[0], Arg::Id(named) if named == *id))
            .and_then(|fact| match &fact.args[2] {
                Arg::Span(key, start, end) => blobs.get(key).and_then(|bytes| bytes.get(*start as usize..*end as usize)),
                _ => None,
            }).map(|bytes| String::from_utf8_lossy(bytes).to_string())
    };
    let pairs: BTreeSet<_> = facts.iter().filter(|fact| fact.relation == "rust.impl")
        .map(|fact| (written(&fact.args[1]), written(&fact.args[2]))).collect();
    serde_json::json!(pairs)
}
