use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use soopy::{
    span_slice, BytePosition, ContentId, ObjectId, Pattern, Revision, SourceSpan, SourceTree,
    SpanPosition, SpanPositionRequest, SpanTextRequest,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn unique(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "soopy_span_{}_{}_{}_{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed),
    ))
}

fn git(root: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_AUTHOR_NAME", "soopy")
        .env("GIT_AUTHOR_EMAIL", "soopy@example.invalid")
        .env("GIT_COMMITTER_NAME", "soopy")
        .env("GIT_COMMITTER_EMAIL", "soopy@example.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repository() -> std::path::PathBuf {
    let root = unique("repository");
    std::fs::create_dir_all(root.join("src")).unwrap();
    git(&root, &["init", "-q"]);
    std::fs::write(root.join("src/text.txt"), "aé\nβ\n").unwrap();
    std::fs::write(root.join("src/empty.txt"), "").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "initial"]);
    root
}

fn text_entry(tree: &mut SourceTree, revision: Revision) -> soopy::SourceEntry {
    entry(tree, revision, "src/text.txt")
}

fn empty_entry(tree: &mut SourceTree, revision: Revision) -> soopy::SourceEntry {
    entry(tree, revision, "src/empty.txt")
}

fn entry(tree: &mut SourceTree, revision: Revision, path: &str) -> soopy::SourceEntry {
    let revision = tree.resolve_revision(revision).unwrap();
    tree.enumerate(&revision, &[Pattern("**/*.txt".into())])
        .unwrap()
        .into_iter()
        .find(|entry| entry.source.path.0.as_ref() == path)
        .unwrap()
}

fn text_request(entry: &soopy::SourceEntry, start: u64, end: u64) -> SpanTextRequest {
    SpanTextRequest {
        span: SourceSpan {
            source: entry.source.clone(),
            start,
            end,
        },
        expected: Some(entry.content.clone()),
    }
}

fn position_request(
    entry: &soopy::SourceEntry,
    start: u64,
    end: u64,
    newline_index_byte_budget: u64,
) -> SpanPositionRequest {
    SpanPositionRequest {
        span: SourceSpan {
            source: entry.source.clone(),
            start,
            end,
        },
        expected: Some(entry.content.clone()),
        newline_index_byte_budget,
    }
}

fn newline_index_storage(line_start_count: usize) -> u64 {
    u64::try_from(line_start_count * std::mem::size_of::<usize>()).unwrap()
}

#[test]
fn batched_span_text_reads_worktree_and_commit_bytes() {
    let root = repository();
    std::fs::write(root.join("src/text.txt"), "xé\nΩ\n").unwrap();
    let mut tree = SourceTree::open(soopy::open(&root).unwrap());
    let commit = text_entry(&mut tree, Revision::Named(Arc::from("HEAD")));
    let worktree = text_entry(&mut tree, Revision::Worktree);

    let text = tree
        .span_text_many(&[
            text_request(&commit, 1, 3),
            text_request(&worktree, 4, 6),
            text_request(&worktree, 7, 7),
        ])
        .unwrap();

    assert_eq!(&*text[0].bytes, "é".as_bytes());
    assert_eq!(&*text[1].bytes, "Ω".as_bytes());
    assert!(text[2].bytes.is_empty());
    assert_eq!(text[0].span.source, commit.source);
    assert_eq!(text[1].span.source, worktree.source);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn positions_use_one_based_lines_and_zero_based_byte_offsets() {
    let root = repository();
    let mut tree = SourceTree::open(soopy::open(&root).unwrap());
    let entry = text_entry(&mut tree, Revision::Named(Arc::from("HEAD")));

    let positions = tree
        .span_position_many(&[
            position_request(&entry, 1, 3, newline_index_storage(3)),
            position_request(&entry, 4, 4, newline_index_storage(3)),
            position_request(&entry, 7, 7, newline_index_storage(3)),
        ])
        .unwrap();

    assert_eq!(
        positions[0],
        SpanPosition {
            span: SourceSpan {
                source: entry.source.clone(),
                start: 1,
                end: 3,
            },
            content: entry.content.clone(),
            start: BytePosition {
                line: 1,
                byte_column: 1,
            },
            end: BytePosition {
                line: 1,
                byte_column: 3,
            },
        }
    );
    assert_eq!(
        positions[1].start,
        BytePosition {
            line: 2,
            byte_column: 0,
        }
    );
    assert_eq!(positions[1].start, positions[1].end);
    assert_eq!(
        positions[2].end,
        BytePosition {
            line: 3,
            byte_column: 0,
        }
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn empty_file_and_terminal_newline_positions_are_explicit() {
    let root = repository();
    let mut tree = SourceTree::open(soopy::open(&root).unwrap());
    let empty = empty_entry(&mut tree, Revision::Named(Arc::from("HEAD")));
    let text = text_entry(&mut tree, Revision::Named(Arc::from("HEAD")));

    let empty_text = tree.span_text_many(&[text_request(&empty, 0, 0)]).unwrap();
    assert!(empty_text[0].bytes.is_empty());
    let positions = tree
        .span_position_many(&[
            position_request(&empty, 0, 0, newline_index_storage(1)),
            position_request(&text, 7, 7, newline_index_storage(3)),
        ])
        .unwrap();
    assert_eq!(
        positions[0].start,
        BytePosition {
            line: 1,
            byte_column: 0,
        }
    );
    assert_eq!(positions[0].start, positions[0].end);
    assert_eq!(
        positions[1].end,
        BytePosition {
            line: 3,
            byte_column: 0,
        }
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn spans_reject_invalid_ranges_and_under_budget_newline_indexes() {
    let root = repository();
    let mut tree = SourceTree::open(soopy::open(&root).unwrap());
    let entry = text_entry(&mut tree, Revision::Worktree);

    assert!(tree.span_text_many(&[text_request(&entry, 5, 4)]).is_err());
    assert!(tree.span_text_many(&[text_request(&entry, 0, 8)]).is_err());
    assert!(tree
        .span_position_many(&[position_request(&entry, 0, 1, newline_index_storage(3) - 1,)])
        .is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn span_slice_preserves_source_for_empty_eof_and_valid_children() {
    let root = repository();
    let mut tree = SourceTree::open(soopy::open(&root).unwrap());
    let entry = text_entry(&mut tree, Revision::Named(Arc::from("HEAD")));
    let parent = SourceSpan {
        source: entry.source.clone(),
        start: 0,
        end: 7,
    };

    assert_eq!(
        span_slice(&parent, 1, 3).unwrap(),
        SourceSpan {
            source: entry.source.clone(),
            start: 1,
            end: 3,
        }
    );
    assert_eq!(
        span_slice(&parent, 4, 4).unwrap(),
        SourceSpan {
            source: entry.source.clone(),
            start: 4,
            end: 4,
        }
    );
    assert_eq!(
        span_slice(&parent, 7, 7).unwrap(),
        SourceSpan {
            source: entry.source.clone(),
            start: 7,
            end: 7,
        }
    );
    assert!(span_slice(&parent, 4, 3).is_err());
    assert!(span_slice(&parent, 0, 8).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn span_requests_reject_wrong_content_and_foreign_worktrees_or_repositories() {
    let first_root = repository();
    let second_root = repository();
    let mut first_tree = SourceTree::open(soopy::open(&first_root).unwrap());
    let mut second_tree = SourceTree::open(soopy::open(&second_root).unwrap());
    let first = text_entry(&mut first_tree, Revision::Worktree);
    let second = text_entry(&mut second_tree, Revision::Worktree);
    let wrong_content = SpanTextRequest {
        span: SourceSpan {
            source: first.source.clone(),
            start: 0,
            end: 1,
        },
        expected: Some(ContentId::GitBlob(ObjectId(Arc::from("wrong")))),
    };
    assert!(first_tree.span_text_many(&[wrong_content]).is_err());
    assert!(first_tree
        .span_text_many(&[text_request(&second, 0, 1)])
        .is_err());

    let linked_root = unique("linked");
    git(
        &first_root,
        &[
            "worktree",
            "add",
            linked_root.to_str().unwrap(),
            "-b",
            "span-linked",
        ],
    );
    let mut linked_tree = SourceTree::open(soopy::open(&linked_root).unwrap());
    let linked = text_entry(&mut linked_tree, Revision::Worktree);
    assert!(first_tree
        .span_text_many(&[text_request(&linked, 0, 1)])
        .is_err());
    git(
        &first_root,
        &[
            "worktree",
            "remove",
            "--force",
            linked_root.to_str().unwrap(),
        ],
    );
    std::fs::remove_dir_all(first_root).unwrap();
    std::fs::remove_dir_all(second_root).unwrap();
}

#[test]
fn mixed_batch_failure_leaves_caller_source_coordinates_unchanged() {
    let first_root = repository();
    let second_root = repository();
    let mut first_tree = SourceTree::open(soopy::open(&first_root).unwrap());
    let mut second_tree = SourceTree::open(soopy::open(&second_root).unwrap());
    let first = text_entry(&mut first_tree, Revision::Worktree);
    let second = text_entry(&mut second_tree, Revision::Worktree);
    let requests = vec![text_request(&first, 0, 1), text_request(&second, 0, 1)];
    let source_coordinates: Vec<_> = requests
        .iter()
        .map(|request| request.span.source.clone())
        .collect();

    assert!(first_tree.span_text_many(&requests).is_err());
    assert_eq!(
        requests
            .iter()
            .map(|request| request.span.source.clone())
            .collect::<Vec<_>>(),
        source_coordinates
    );
    std::fs::remove_dir_all(first_root).unwrap();
    std::fs::remove_dir_all(second_root).unwrap();
}

fn round_trip<T: serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug + PartialEq>(
    value: &T,
) {
    let json = serde_json::to_string(value).unwrap();
    let back: T = serde_json::from_str(&json).unwrap();
    assert_eq!(&back, value);
}

#[test]
fn public_span_requests_and_results_round_trip_through_json() {
    let root = repository();
    let mut tree = SourceTree::open(soopy::open(&root).unwrap());
    let entry = text_entry(&mut tree, Revision::Named(Arc::from("HEAD")));
    let request = text_request(&entry, 1, 3);
    let text = tree
        .span_text_many(std::slice::from_ref(&request))
        .unwrap()
        .pop()
        .unwrap();
    let position_request = position_request(&entry, 1, 3, newline_index_storage(3));
    let position = tree
        .span_position_many(std::slice::from_ref(&position_request))
        .unwrap()
        .pop()
        .unwrap();

    round_trip(&request);
    round_trip(&text);
    round_trip(&position_request);
    round_trip(&position);
    std::fs::remove_dir_all(root).unwrap();
}
