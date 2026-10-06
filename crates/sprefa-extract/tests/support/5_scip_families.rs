use serde_json::{json, Value};

pub fn evaluate(case: &Value) -> Value {
    crate::fixture_runner::commands(case, |step| {
        assert_eq!(step["api"], "indexer_roster");
        json!(sprefa_extract::INDEXERS.iter().map(|indexer| {
            assert_eq!(indexer.source.indexer(),indexer.bin);
            assert!(!indexer.markers.is_empty());
            assert!(!indexer.install.is_empty());
            json!({"language":indexer.lang,"binary":indexer.bin,"source_binary":indexer.source.indexer(),"markers":indexer.markers,"install":indexer.install})
        }).collect::<Vec<_>>())
    })
}
