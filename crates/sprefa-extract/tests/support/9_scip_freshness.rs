use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;
use sprefa_extract::{IndexSet, IndexBudget};

pub fn evaluate(case: &Value) -> Value {
    let sets = RefCell::new(BTreeMap::<String, IndexSet>::new());
    crate::fixture_runner::commands(case, |step| {
        let name = step["name"].as_str().unwrap();
        let path = |field: &str| Path::new(step[field].as_str().unwrap());
        let result = match step["api"].as_str().unwrap() {
            "define_set" | "source_set" => {
                let set = if step["api"] == "source_set" { sprefa_extract::source_set_for_root(path("root")).unwrap() }
                else { IndexSet::new(step["entries"].as_array().unwrap().iter().map(|pair| (pair[0].as_str().unwrap().to_string(), pair[1].as_str().unwrap().to_string()))) };
                let result = json!({"len":set.len(),"digest":set.digest(),"paths":set.paths().collect::<Vec<_>>()});
                if let Some(length) = step["expect_length"].as_u64() { assert_eq!(set.len(), length as usize); }
                sets.borrow_mut().insert(name.to_string(), set);
                result
            }
            "digest_compare" => {
                let sets = sets.borrow();
                json!(sets[step["left"].as_str().unwrap()].digest() == sets[step["right"].as_str().unwrap()].digest())
            }
            "record_set" => { sprefa_extract::record_index_set(path("index"), &sets.borrow()[step["set"].as_str().unwrap()]); Value::Null }
            "lookup" | "legacy_lookup" | "fresh" => {
                let sets = sets.borrow();
                let set = step["set"].as_str().map(|key| &sets[key]);
                let digest = set.map(IndexSet::digest).or_else(|| step["digest"].as_str());
                let previous = std::env::var_os("SPREFA_SCIP_INDEX");
                if let Some(index) = step["override"].as_str() { std::env::set_var("SPREFA_SCIP_INDEX", index); }
                let found = if step["api"] == "fresh" { sprefa_extract::fresh_index_for_set(path("root"), digest.unwrap()) }
                    else if step["api"] == "legacy_lookup" { sprefa_extract::index_path(path("root"), path("cache")) }
                    else { sprefa_extract::index_path_for_set(path("root"), path("cache"), digest) };
                match previous { Some(index) => std::env::set_var("SPREFA_SCIP_INDEX", index), None => std::env::remove_var("SPREFA_SCIP_INDEX") }
                json!(found.map(|file| file.to_string_lossy().into_owned()))
            }
            "ensure" => {
                let sets = sets.borrow();
                let set = step["set"].as_str().map(|key| &sets[key]);
                let previous = std::env::var_os("PATH");
                if let Some(value) = step["path"].as_str() { std::env::set_var("PATH", crate::fixture_runner::expand_text(value, "")); }
                let started = std::time::Instant::now();
                let report = sprefa_extract::ensure_index_for_set(path("root"), path("cache"), IndexBudget { secs:step["budget_seconds"].as_u64().unwrap() }, set);
                let elapsed = started.elapsed();
                match previous { Some(value) => std::env::set_var("PATH", value), None => std::env::remove_var("PATH") }
                if let Some(limit) = step["limit_seconds"].as_u64() { crate::wall_bench::check(name, elapsed.as_secs() as f64, limit as f64, false); }
                if let Some(expected) = step["expect_timed_out_seconds"].as_u64() { assert!(report.skips.iter().any(|skip| matches!(skip.reason, sprefa_extract::SkipReason::TimedOut { secs } if secs == expected))); }
                if let Some(reused) = step["expect_reused"].as_bool() { assert_eq!(report.reused, reused); }
                if step["expect_no_index"] == true { assert!(report.index.is_none()); }
                if let Some(reason) = step["expect_first_skip"].as_str() { assert_eq!(report.skips.first().map(|skip| skip.reason.slug()), Some(reason)); }
                json!({"reused":report.reused,"index":report.index.map(|file| file.to_string_lossy().into_owned()),"skips":report.skips.iter().map(|skip| skip.reason.slug()).collect::<Vec<_>>(),"budget":{"seconds":step["budget_seconds"],"wait_under_seconds":step["limit_seconds"],"timed_out_seconds":step["expect_timed_out_seconds"]}})
            }
            "copy_sources" => {
                let strings = |key: &str| step[key].as_array().unwrap().iter().map(|value| value.as_str().unwrap()).collect::<Vec<_>>();
                sprefa_extract::copy_sources(path("root"), path("stage"), &strings("extensions"), &strings("markers")).unwrap();
                Value::Null
            }
            "span_tables" => span_tables(step),
            other => panic!("unknown freshness API: {other}"),
        };
        if let Some(expected) = step.get("expect") { assert_eq!(&result, expected, "{step}"); }
        result
    })
}

fn span_tables(step: &Value) -> Value {
    use sprefa_extract::types::{OccurrenceRole, PositionEncoding, ScipDocument, ScipIndex, ScipOccurrence, SymbolInterner};
    let inputs = step["documents"].as_array().unwrap();
    let range: [i32;4] = serde_json::from_value(step["range"].clone()).unwrap();
    let site = sprefa_extract::shape::Span { start:step["site"]["start"].as_u64().unwrap() as u32, len:step["site"]["len"].as_u64().unwrap() as u32 };
    let indexes: Vec<_> = inputs.iter().map(|input| {
        let mut interner = SymbolInterner::default();
        let text = input["symbol"].as_str().unwrap();
        let symbol = interner.intern(text);
        let doc = ScipDocument { relative_path:step["relative_path"].as_str().unwrap().to_string(), position_encoding:PositionEncoding::Utf8,
            occurrences:vec![ScipOccurrence { symbol, range, roles:OccurrenceRole(step["roles"].as_i64().unwrap() as i32), syntax_kind:0,
                enclosing_range:None, override_documentation:Vec::new(), diagnostics:Vec::new() }], ..ScipDocument::default() };
        ScipIndex { documents:vec![doc], symbols:vec![text.to_string()], ..ScipIndex::default() }
    }).collect();
    let answers: Vec<_> = step["requests"].as_array().unwrap().iter().map(|request| {
        let index = request.as_u64().unwrap() as usize;
        let input = &inputs[index];
        let symbol = sprefa_extract::site_occurrence(&indexes[index].documents[0], input["content"].as_str().unwrap().as_bytes(), site, input["name"].as_str().unwrap()).unwrap();
        assert_eq!(symbol, indexes[index].documents[0].occurrences[0].symbol);
        indexes[index].symbol(symbol).to_string()
    }).collect();
    let tables: Vec<Vec<_>> = indexes.iter().map(|index| index.documents[0].spans.get().unwrap().spans.iter().map(|&(start,end,symbol)| json!({"start":start,"end":end,"symbol":index.symbol(symbol)})).collect()).collect();
    assert_ne!(tables[0], tables[1]);
    json!({"answers":answers,"tables":tables})
}
