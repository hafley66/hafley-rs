//! The soopy edit fold: byte spans into the ONE Replace per file, and the
//! expected-hash precondition soopy checks before it derives any output.
//!
//! @comment-ok: sabotage receipt, repo law keeps these in TEST headers.
//! SABOTAGE (when the fold was `From<BoundEdit>`): the conversion rewritten to
//! `end: start` (deleted_length dropped, every edit a pure insertion) measured
//! 4 failed / 4 passed, and `stage_edits_reaches_soopy_with_the_folded_spans`
//! was one of the GREEN ones: reaching soopy judges nothing about span width,
//! so the (start, end, bytes) assertions are what catch it. The conversion is
//! gone (the ast-grep drain it served is gone); the fold + stage receipts stay.
//! FAIL-FIRST: `stale_expected_is_refused_by_stage` with `expected` hashed from
//! the real on-disk bytes measured `Ok` at its `expect_err`, so it fails only
//! on the wrong hash, which is the precondition the whole drain rests on.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sprefa_extract::{directory_source, replace_action, stage_edits};

const SRC: &str = "fn main() { foo(); foo(); }\n";
const REL: &str = "src/main.rs";

fn producer() -> soopy::ActionProducer {
    soopy::ActionProducer::unordered("test-drain")
}

fn detached_identity() -> soopy::DirectoryId {
    soopy::DirectoryId(Arc::from("test-directory"))
}

/// The old drain's edit shape, built directly: (12, 17) and (19, 24) are the
/// two `foo()` call spans in `SRC`.
fn edit_at(source: &soopy::ActionSource, start: u64, replacement: &[u8]) -> soopy::TextEdit {
    soopy::TextEdit {
        range: soopy::ActionSpan {
            source: source.clone(),
            start,
            end: start + 5,
        },
        replacement: replacement.to_vec(),
        producer: producer(),
    }
}

fn spans(action: &soopy::SourceAction) -> Vec<(u64, u64, String)> {
    let soopy::SourceAction::Replace { edits, .. } = action else {
        panic!("not a Replace: {action:?}");
    };
    edits
        .iter()
        .map(|edit| {
            (
                edit.range.start,
                edit.range.end,
                String::from_utf8(edit.replacement.clone()).unwrap(),
            )
        })
        .collect()
}

fn temp_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "extract_drain_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join(REL), SRC).unwrap();
    root.canonicalize().unwrap()
}

fn identity_of(root: &Path) -> soopy::DirectoryId {
    soopy::SourceRoot::open_directory(root)
        .unwrap()
        .directory()
        .identity
        .clone()
}

#[test]
fn edits_fold_into_one_replace_action_per_file() {
    let source = directory_source(&detached_identity(), REL);
    let action = replace_action(
        source.clone(),
        soopy::ContentId::blake3(SRC.as_bytes()),
        vec![
            edit_at(&source, 12, b"bar()"),
            edit_at(&source, 19, b"bar()"),
        ],
    );

    let soopy::SourceAction::Replace {
        source: action_source,
        expected,
        ..
    } = &action
    else {
        panic!("not a Replace: {action:?}");
    };
    assert_eq!(action_source, &source);
    assert_eq!(expected, &soopy::ContentId::blake3(SRC.as_bytes()));
    assert_eq!(
        spans(&action),
        vec![(12, 17, "bar()".to_string()), (19, 24, "bar()".to_string()),],
        "src: {SRC:?}"
    );
}

#[test]
fn duplicate_spans_collapse_to_one_edit() {
    let source = directory_source(&detached_identity(), REL);
    let action = replace_action(
        source.clone(),
        soopy::ContentId::blake3(SRC.as_bytes()),
        vec![edit_at(&source, 12, b"bar()"), edit_at(&source, 12, b"bar()")],
    );

    assert_eq!(spans(&action), vec![(12, 17, "bar()".to_string())]);
}

#[test]
fn stage_edits_reaches_soopy_with_the_folded_spans() {
    let root = temp_root("stage");
    let identity = identity_of(&root);
    let source = directory_source(&identity, REL);
    let request = stage_edits(
        source.clone(),
        soopy::ContentId::blake3(SRC.as_bytes()),
        vec![
            edit_at(&source, 12, b"bar()"),
            edit_at(&source, 19, b"bar()"),
        ],
        soopy::SourceRootId::Directory {
            directory: identity,
        },
    );
    request.validate_shape().expect("shape holds");

    let mut source_root = soopy::SourceRoot::open_directory(&root).unwrap();
    let mut store = soopy::InMemoryStageStore::new();
    let staged = soopy::stage_mutations(&mut source_root, &request, &mut store).expect("staged");
    assert_eq!(staged.previews.len(), 1);

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn stale_expected_is_refused_by_stage() {
    let root = temp_root("stale");
    let identity = identity_of(&root);
    let source = directory_source(&identity, REL);
    let stale = soopy::ContentId::blake3(b"not what is on disk");
    let request = stage_edits(
        source.clone(),
        stale.clone(),
        vec![
            edit_at(&source, 12, b"bar()"),
            edit_at(&source, 19, b"bar()"),
        ],
        soopy::SourceRootId::Directory {
            directory: identity,
        },
    );

    let mut source_root = soopy::SourceRoot::open_directory(&root).unwrap();
    let mut store = soopy::InMemoryStageStore::new();
    let refusal = soopy::stage_mutations(&mut source_root, &request, &mut store)
        .expect_err("a wrong expected hash cannot stage");

    let soopy::StageRefusal::Stale { inputs } = refusal else {
        panic!("expected a Stale refusal, got {refusal:?}");
    };
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].source, source);
    assert_eq!(inputs[0].expected, stale);
    assert_eq!(
        inputs[0].observed,
        Some(soopy::ContentId::blake3(SRC.as_bytes()))
    );

    std::fs::remove_dir_all(&root).unwrap();
}
