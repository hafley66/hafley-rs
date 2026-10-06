use serde_json::{json, Value};
use sprefa_extract::tsi::{relation, Arg, Method, TsiSink, REGISTRY};

pub fn evaluate(case: &Value) -> Value {
    crate::fixture_runner::commands(case, |step| {
        if step["api"] == "registry" {
            let schema = std::fs::read_to_string(step["path"].as_str().unwrap()).unwrap();
            let rows: Vec<_> = schema.lines().filter(|line| line.trim_start().starts_with("relation=")).collect();
            assert_eq!(rows.len(), REGISTRY.len());
            let expected: Vec<_> = REGISTRY.iter().map(|relation| format!("  relation={} args=[{}]", relation.name, relation.args.iter().map(|kind| kind.word()).collect::<Vec<_>>().join(","))).collect();
            for row in &expected { assert!(rows.contains(&row.as_str()), "missing {row}"); }
            return json!(expected);
        }
        let mut sink = TsiSink::new(step["run"].as_u64().unwrap() as u32, serde_json::from_value::<Method>(step["method"].clone()).unwrap());
        let ids: Vec<_> = (0..step["fresh_ids"].as_u64().unwrap_or(0)).map(|_| sink.fresh_id()).collect();
        let facts = || step["facts"].as_array().unwrap().iter().map(|fact| (relation(fact["relation"].as_str().unwrap()).unwrap().name, serde_json::from_value::<Vec<Arg>>(fact["args"].clone()).unwrap()));
        if step["api"] == "wrong_arity" {
            if cfg!(debug_assertions) {
                let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { for (relation, args) in facts() { sink.fact(relation, args); } })).expect_err("the sink accepted the wrong arity");
                let message = panic.downcast_ref::<String>().map(String::as_str).or_else(|| panic.downcast_ref::<&str>().copied()).unwrap();
                assert!(message.contains(step["message"].as_str().unwrap()), "{message}");
            }
            return json!({"debug_only":true,"expected_message":step["message"]});
        }
        let ordinals: Vec<_> = facts().map(|(relation, args)| sink.fact(relation, args)).collect();
        for name in step["complete"].as_array().unwrap() { sink.complete(relation(name.as_str().unwrap()).unwrap().name); }
        json!({"ids":ids,"ordinals":ordinals,"rows":sink.rows()})
    })
}
