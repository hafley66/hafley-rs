use serde_json::{json, Value};
use sprefa_extract::{FlatFact, tsi::{Arg, relation}};
use std::collections::{BTreeMap, BTreeSet};

pub fn evaluate(case: &Value) -> Value {
    crate::fixture_runner::commands(case, |step| {
        let rows: Vec<FlatFact> = std::fs::read_to_string(step["input"].as_str().unwrap()).unwrap().lines().map(|line| serde_json::from_str(line).unwrap()).collect();
        let facts: Vec<_> = rows.iter().filter_map(|row| if let FlatFact::Fact(fact) = row { Some(fact) } else { None }).collect();
        assert!(!facts.is_empty());
        let mut declared = BTreeSet::new();
        for fact in &facts {
            assert_eq!(fact.args.len(), relation(&fact.relation).unwrap().args.len());
            let position = match fact.relation.as_str() { "tsi.type" | "tsi.edge" => 0, "tsi.called" => 2, _ => continue };
            if let Arg::Id(id) = fact.args[position] { declared.insert(id); }
        }
        for fact in &facts {
            for arg in &fact.args { if let Arg::Id(id) = arg { assert!(declared.contains(id)); } }
        }
        let emitted: BTreeSet<_> = facts.iter().map(|fact| fact.relation.as_str()).collect();
        let coverage: BTreeMap<_,_> = rows.iter().filter_map(|row| if let FlatFact::Coverage(claim) = row { Some((claim.relation.as_str(), claim.complete)) } else { None }).collect();
        for name in &emitted { assert_eq!(coverage.get(name), Some(&false)); }
        for (name, complete) in &coverage { assert!(!complete); if !name.starts_with("extract.") { assert!(emitted.contains(name)); } }
        json!({"fact_rows":facts.len(),"declared_ids":declared.len(),"relations":emitted,"coverage":coverage})
    })
}
