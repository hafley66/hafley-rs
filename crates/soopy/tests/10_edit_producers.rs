use std::sync::Arc;

use soopy::{
    deduplicate_equivalent_edits, from_ast_grep_parts, ActionProducer, ActionSource, ActionSpan,
    BiomeBatchMutationContract, DirectoryId, FileRef, ProducedEdit, ProducedEditBatch,
    ProducedEditBatchValidationError, RootPath, TextEdit, Utf8TextEdit,
};

fn range(start: u64, end: u64) -> ActionSpan {
    ActionSpan {
        source: ActionSource::Directory {
            file: FileRef {
                directory: DirectoryId(Arc::from("repo")),
                path: RootPath(Arc::from("src/lib.rs")),
            },
        },
        start,
        end,
    }
}
#[test]
fn equivalent_edits_deduplicate_with_all_provenance() {
    let edit = ProducedEdit::new(
        range(2, 5),
        b"new".to_vec(),
        ActionProducer::unordered("ast-grep").with_rule("rename-call"),
    );
    let same = ProducedEdit::new(
        range(2, 5),
        b"new".to_vec(),
        ActionProducer::unordered("dl6").with_rule("rename-call"),
    );
    let different = ProducedEdit::new(
        range(2, 5),
        b"other".to_vec(),
        ActionProducer::unordered("other-rule"),
    );

    let deduplicated = deduplicate_equivalent_edits([edit, same, different]);
    assert_eq!(deduplicated.len(), 2);
    assert_eq!(deduplicated[0].producers.len(), 2);
    assert_eq!(deduplicated[1].producers.len(), 1);
    assert_eq!(
        serde_json::to_string(&deduplicated).unwrap(),
        r#"[{"range":{"source":{"kind":"directory","file":{"directory":"repo","path":"src/lib.rs"}},"start":2,"end":5},"replacement":[110,101,119],"producers":[{"id":"ast-grep","same_offset_order":null,"rule":"rename-call"},{"id":"dl6","same_offset_order":null,"rule":"rename-call"}]},{"range":{"source":{"kind":"directory","file":{"directory":"repo","path":"src/lib.rs"}},"start":2,"end":5},"replacement":[111,116,104,101,114],"producers":[{"id":"other-rule","same_offset_order":null}]}]"#
    );
}

#[test]
fn ast_grep_parts_reject_offset_overflow_deterministically() {
    assert_eq!(
        from_ast_grep_parts(
            range(0, 0).source,
            usize::MAX,
            1,
            Vec::<u8>::new(),
            ActionProducer::unordered("ast-grep"),
        ),
        Err(soopy::ProducedEditConversionError::AstGrepOffsetOverflow)
    );
}

#[test]
fn byte_and_utf8_adapters_keep_rule_and_producer_provenance() {
    let utf8 = Utf8TextEdit {
        range: range(4, 6),
        replacement: "é".to_owned(),
        producer: ActionProducer::ordered("rust-analyzer", 2).with_rule("extract"),
    };
    let produced = ProducedEdit::from_utf8_text_edit(utf8);
    assert_eq!(produced.replacement, "é".as_bytes());
    assert_eq!(produced.producers[0].id, "rust-analyzer");
    assert_eq!(produced.producers[0].rule.as_deref(), Some("extract"));

    let text: TextEdit = soopy::ProducedEditBatch::new(vec![produced])
        .into_text_edits()
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(text.range.start, 4);
    assert_eq!(text.replacement, "é".as_bytes());
}

#[test]
fn producer_batch_validation_reports_schema_and_empty_producers_in_order() {
    let batch = ProducedEditBatch {
        schema_version: 99,
        edits: vec![ProducedEdit {
            range: range(0, 0),
            replacement: Vec::new(),
            producers: Vec::new(),
        }],
    };
    assert_eq!(
        batch.validate(),
        Err(vec![
            ProducedEditBatchValidationError::UnsupportedSchemaVersion { found: 99 },
            ProducedEditBatchValidationError::EmptyProducers { edit_index: 0 },
        ])
    );
}

#[test]
fn biome_contract_fixture_is_deterministic_and_has_no_runtime_dependency() {
    let contract = BiomeBatchMutationContract {
        range: range(10, 12),
        replacement: "é".to_owned(),
        producer: ActionProducer::ordered("biome", 4).with_rule("use-const"),
    };
    let json = serde_json::to_string(&contract).unwrap();
    assert_eq!(
        json,
        r#"{"range":{"source":{"kind":"directory","file":{"directory":"repo","path":"src/lib.rs"}},"start":10,"end":12},"replacement":"é","producer":{"id":"biome","same_offset_order":4,"rule":"use-const"}}"#
    );
    let produced: ProducedEdit = contract.into();
    assert_eq!(produced.replacement, "é".as_bytes());
    assert_eq!(produced.producers[0].id, "biome");
}
