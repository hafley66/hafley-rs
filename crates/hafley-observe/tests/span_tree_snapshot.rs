use serde_json::Value;
use test_span::prelude::*;

#[test_span]
#[level(tracing::Level::DEBUG)]
fn nested_span_tree_snapshots_child_counts() {
    let populate = tracing::debug_span!("populate");
    let _populate_guard = populate.enter();
    for row in 0..3 {
        let maintain = tracing::debug_span!("maintain", row = row);
        let _maintain_guard = maintain.enter();
    }

    let raw = serde_json::to_string(&get_spans()).unwrap();
    let tree: Value = serde_json::from_str(&raw).unwrap();

    let populate_child = &tree["children"]["span_tree_snapshot::populate"];
    assert_eq!(
        populate_child["name"],
        "span_tree_snapshot::populate",
        "parent span missing from the tree"
    );
    for row in 0..3 {
        let row_entry = format!("\"row\",{row}");
        assert!(
            raw.contains(&row_entry),
            "maintain instance {row} missing from the tree"
        );
    }

    let maintain_children = populate_child["children"]
        .as_object()
        .expect("children serialize as a map keyed by span name");
    assert_eq!(
        maintain_children.len(),
        1,
        "3 maintain siblings entered, serialized map keys collapse them to 1, child counts are not queryable"
    );
}
