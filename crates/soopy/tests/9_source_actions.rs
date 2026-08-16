use std::sync::Arc;

use soopy::{
    ActionProducer, ActionSource, ActionSpan, ContentId, DirectoryId, FileRef, ObjectId, RepoPath,
    RepositoryId, RevisionId, RootPath, SourceAction, SourceActionValidationError, SourcePath,
    SourceRef, SourceRootId, StageRequest, TextEdit, Utf8TextEdit, WorktreeId,
    SOURCE_ACTION_SCHEMA_VERSION,
};

fn git_root() -> SourceRootId {
    SourceRootId::GitWorktree {
        repository: RepositoryId(Arc::from("repository")),
        worktree: WorktreeId(Arc::from("worktree")),
    }
}

fn git_source(revision: RevisionId) -> SourceRef {
    SourceRef {
        repository: RepositoryId(Arc::from("repository")),
        revision,
        path: RepoPath(Arc::from("src/lib.rs")),
    }
}

fn worktree_source() -> SourceRef {
    git_source(RevisionId::Worktree {
        worktree: WorktreeId(Arc::from("worktree")),
        head: Some(ObjectId(Arc::from("head"))),
        dirty: true,
    })
}

fn content(byte: u8) -> ContentId {
    ContentId::Blake3([byte; 32])
}

fn byte_edit(start: u64, end: u64, replacement: &[u8], producer: ActionProducer) -> TextEdit {
    TextEdit {
        range: ActionSpan {
            source: ActionSource::Git {
                source: worktree_source(),
            },
            start,
            end,
        },
        replacement: replacement.to_vec(),
        producer,
    }
}

#[test]
fn versioned_git_worktree_request_round_trips_with_stable_json() {
    let utf8: TextEdit = Utf8TextEdit {
        range: soopy::SourceSpan {
            source: worktree_source(),
            start: 4,
            end: 6,
        }
        .into(),
        replacement: "Ω".into(),
        producer: ActionProducer::ordered("rust-analyzer:assist", 7),
    }
    .into();
    let request = StageRequest::new(
        git_root(),
        vec![SourceAction::Replace {
            source: ActionSource::Git {
                source: worktree_source(),
            },
            expected: content(1),
            edits: vec![
                byte_edit(1, 3, &[0xff, 0x00], ActionProducer::unordered("generator")),
                utf8,
            ],
        }],
    );

    request.validate_shape().unwrap();
    let json = serde_json::to_string(&request).unwrap();
    assert_eq!(
        json,
        r#"{"schema_version":1,"root":{"kind":"git_worktree","repository":"repository","worktree":"worktree"},"actions":[{"kind":"replace","source":{"kind":"git","source":{"repository":"repository","revision":{"Worktree":{"worktree":"worktree","head":"head","dirty":true}},"path":"src/lib.rs"}},"expected":{"Blake3":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1]},"edits":[{"range":{"source":{"kind":"git","source":{"repository":"repository","revision":{"Worktree":{"worktree":"worktree","head":"head","dirty":true}},"path":"src/lib.rs"}},"start":1,"end":3},"replacement":[255,0],"producer":{"id":"generator","same_offset_order":null}},{"range":{"source":{"kind":"git","source":{"repository":"repository","revision":{"Worktree":{"worktree":"worktree","head":"head","dirty":true}},"path":"src/lib.rs"}},"start":4,"end":6},"replacement":[206,169],"producer":{"id":"rust-analyzer:assist","same_offset_order":7}}]}]}"#
    );
    let round_trip: StageRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(round_trip, request);
    assert_eq!(serde_json::to_string(&round_trip).unwrap(), json);
}

#[test]
fn plain_directory_create_move_and_delete_preserve_directory_identity() {
    let directory = DirectoryId(Arc::from("directory"));
    let root = SourceRootId::Directory {
        directory: directory.clone(),
    };
    let source = ActionSource::Directory {
        file: FileRef {
            directory: directory.clone(),
            path: RootPath(Arc::from("old.bin")),
        },
    };
    let request = StageRequest::new(
        root.clone(),
        vec![
            SourceAction::Create {
                path: SourcePath::Directory {
                    path: RootPath(Arc::from("new.bin")),
                },
                bytes: vec![0, 255],
            },
            SourceAction::Move {
                source: source.clone(),
                expected: content(2),
                destination: SourcePath::Directory {
                    path: RootPath(Arc::from("moved.bin")),
                },
            },
            SourceAction::Delete {
                source,
                expected: content(3),
            },
        ],
    );

    request.validate_shape().unwrap();
    let json = serde_json::to_string(&request).unwrap();
    assert_eq!(
        json,
        r#"{"schema_version":1,"root":{"kind":"directory","directory":"directory"},"actions":[{"kind":"create","path":{"kind":"directory","path":"new.bin"},"bytes":[0,255]},{"kind":"move","source":{"kind":"directory","file":{"directory":"directory","path":"old.bin"}},"expected":{"Blake3":[2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2]},"destination":{"kind":"directory","path":"moved.bin"}},{"kind":"delete","source":{"kind":"directory","file":{"directory":"directory","path":"old.bin"}},"expected":{"Blake3":[3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3]}}]}"#
    );
    assert_eq!(
        serde_json::from_str::<StageRequest>(&json).unwrap(),
        request
    );
}

#[test]
fn utf8_edit_adapter_accepts_plain_directory_ranges() {
    let range = ActionSpan {
        source: ActionSource::Directory {
            file: FileRef {
                directory: DirectoryId(Arc::from("directory")),
                path: RootPath(Arc::from("text.txt")),
            },
        },
        start: 1,
        end: 3,
    };
    let edit: TextEdit = Utf8TextEdit {
        range: range.clone(),
        replacement: "é".into(),
        producer: ActionProducer::unordered("utf8-producer"),
    }
    .into();

    assert_eq!(edit.range, range);
    assert_eq!(edit.replacement, "é".as_bytes());
}

#[test]
fn immutable_commit_source_is_rejected_as_a_mutation_target() {
    let commit_source = git_source(RevisionId::Commit(ObjectId(Arc::from("commit"))));
    let request = StageRequest::new(
        git_root(),
        vec![SourceAction::Delete {
            source: ActionSource::Git {
                source: commit_source.clone(),
            },
            expected: ContentId::GitBlob(ObjectId(Arc::from("blob"))),
        }],
    );

    assert_eq!(
        request.validate_shape(),
        Err(vec![SourceActionValidationError::ImmutableGitSource {
            action_index: 0,
            source: commit_source,
        }])
    );
}

#[test]
fn request_shape_rejects_wrong_root_edit_source_and_inverted_range() {
    let foreign_source = ActionSource::Directory {
        file: FileRef {
            directory: DirectoryId(Arc::from("foreign-directory")),
            path: RootPath(Arc::from("wrong.txt")),
        },
    };
    let target = ActionSource::Git {
        source: worktree_source(),
    };
    let request = StageRequest {
        schema_version: SOURCE_ACTION_SCHEMA_VERSION + 1,
        root: git_root(),
        actions: vec![
            SourceAction::Create {
                path: SourcePath::Directory {
                    path: RootPath(Arc::from("wrong.txt")),
                },
                bytes: Vec::new(),
            },
            SourceAction::Replace {
                source: target.clone(),
                expected: content(4),
                edits: vec![TextEdit {
                    range: ActionSpan {
                        source: foreign_source.clone(),
                        start: 9,
                        end: 8,
                    },
                    replacement: Vec::new(),
                    producer: ActionProducer::unordered("fixture"),
                }],
            },
        ],
    };

    assert_eq!(
        request.validate_shape(),
        Err(vec![
            SourceActionValidationError::UnsupportedSchemaVersion {
                found: SOURCE_ACTION_SCHEMA_VERSION + 1,
            },
            SourceActionValidationError::PathRootMismatch {
                action_index: 0,
                path: SourcePath::Directory {
                    path: RootPath(Arc::from("wrong.txt")),
                },
                root: git_root(),
            },
            SourceActionValidationError::EditSourceMismatch {
                action_index: 1,
                edit_index: 0,
                action_source: target,
                edit_source: foreign_source,
            },
            SourceActionValidationError::InvertedEditRange {
                action_index: 1,
                edit_index: 0,
                start: 9,
                end: 8,
            },
        ])
    );
}

#[test]
fn planner_cases_remain_lossless_request_fixtures() {
    let request = StageRequest::new(
        git_root(),
        vec![SourceAction::Replace {
            source: ActionSource::Git {
                source: worktree_source(),
            },
            // This deliberately stale precondition remains an action input;
            // only a stage snapshot can compare it with current bytes.
            expected: content(99),
            edits: vec![
                // Two unordered inserts at the same offset are retained for
                // the planner to refuse.
                byte_edit(0, 0, b"first", ActionProducer::unordered("one")),
                byte_edit(0, 0, b"second", ActionProducer::unordered("two")),
                // Explicit ordering is preserved independently of producer ID.
                byte_edit(4, 4, b"before", ActionProducer::ordered("z", 1)),
                byte_edit(4, 4, b"after", ActionProducer::ordered("a", 2)),
                // Adjacent edits remain distinct original-byte ranges.
                byte_edit(8, 10, b"left", ActionProducer::unordered("adjacent-a")),
                byte_edit(10, 12, b"right", ActionProducer::unordered("adjacent-b")),
                // This overlap is also retained for planner conflict handling.
                byte_edit(14, 18, b"outer", ActionProducer::unordered("overlap-a")),
                byte_edit(16, 20, b"inner", ActionProducer::unordered("overlap-b")),
            ],
        }],
    );

    request.validate_shape().unwrap();
    let SourceAction::Replace {
        expected, edits, ..
    } = &request.actions[0]
    else {
        panic!("fixture action changed kind");
    };
    assert_eq!(expected, &content(99));
    assert_eq!(
        edits
            .iter()
            .map(|edit| {
                (
                    edit.range.start,
                    edit.range.end,
                    edit.producer.same_offset_order,
                )
            })
            .collect::<Vec<_>>(),
        vec![
            (0, 0, None),
            (0, 0, None),
            (4, 4, Some(1)),
            (4, 4, Some(2)),
            (8, 10, None),
            (10, 12, None),
            (14, 18, None),
            (16, 20, None),
        ]
    );
    let json = serde_json::to_string(&request).unwrap();
    assert_eq!(
        serde_json::to_string(&serde_json::from_str::<StageRequest>(&json).unwrap()).unwrap(),
        json
    );
}
