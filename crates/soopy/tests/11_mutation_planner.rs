use std::sync::Arc;

use soopy::{
    plan_mutations, ActionProducer, ActionSource, ActionSpan, ContentId, FileRef, MutationConflict,
    PlannedFileKind, RepoPath, RootPath, SourceAction, SourcePath, SourceRef, SourceRoot,
    SourceRootId, StageRefusal, StageRequest, TextEdit,
};

fn temporary_root(label: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "soopy_mutation_planner_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn content(bytes: &[u8]) -> ContentId {
    ContentId::Blake3(*blake3::hash(bytes).as_bytes())
}

fn directory_source(root: &SourceRoot, path: &str) -> ActionSource {
    ActionSource::Directory {
        file: FileRef {
            directory: root.directory().identity.clone(),
            path: RootPath(Arc::from(path)),
        },
    }
}

fn directory_path(path: &str) -> SourcePath {
    SourcePath::Directory {
        path: RootPath(Arc::from(path)),
    }
}

fn edit(
    source: ActionSource,
    start: u64,
    end: u64,
    replacement: &[u8],
    producer: ActionProducer,
) -> TextEdit {
    TextEdit {
        range: ActionSpan { source, start, end },
        replacement: replacement.to_vec(),
        producer,
    }
}

#[test]
fn planner_normalizes_input_order_and_splices_original_coordinates_descending() {
    let directory = temporary_root("normalizes");
    std::fs::write(directory.join("source.txt"), b"0123456789").unwrap();
    let mut root = SourceRoot::open_directory(&directory).unwrap();
    let root_id = SourceRootId::Directory {
        directory: root.directory().identity.clone(),
    };
    let source = directory_source(&root, "source.txt");
    let expected = content(b"0123456789");
    let first = SourceAction::Replace {
        source: source.clone(),
        expected: expected.clone(),
        edits: vec![
            edit(
                source.clone(),
                6,
                8,
                b"AB",
                ActionProducer::unordered("range-b"),
            ),
            edit(
                source.clone(),
                0,
                0,
                b"second",
                ActionProducer::ordered("second", 2),
            ),
        ],
    };
    let second = SourceAction::Replace {
        source: source.clone(),
        expected,
        edits: vec![
            edit(
                source.clone(),
                2,
                4,
                b"XX",
                ActionProducer::unordered("range-a"),
            ),
            edit(source, 0, 0, b"first", ActionProducer::ordered("first", 1)),
        ],
    };

    let forward = plan_mutations(
        &mut root,
        &StageRequest::new(root_id.clone(), vec![first.clone(), second.clone()]),
    )
    .unwrap();
    let reverse =
        plan_mutations(&mut root, &StageRequest::new(root_id, vec![second, first])).unwrap();

    assert_eq!(forward, reverse);
    assert_eq!(forward.files.len(), 1);
    assert_eq!(forward.files[0].kind, PlannedFileKind::Replace);
    assert_eq!(
        forward.files[0].bytes_after.as_deref(),
        Some(b"firstsecond01XX45AB89".as_slice())
    );
    assert_eq!(
        forward.files[0]
            .edits
            .iter()
            .map(|edit| (edit.start, edit.end, edit.replacement.clone()))
            .collect::<Vec<_>>(),
        vec![
            (0, 0, b"first".to_vec()),
            (0, 0, b"second".to_vec()),
            (2, 4, b"XX".to_vec()),
            (6, 8, b"AB".to_vec()),
        ]
    );
    assert_eq!(
        std::fs::read(directory.join("source.txt")).unwrap(),
        b"0123456789"
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn planner_refuses_stale_input_before_returning_a_plan() {
    let directory = temporary_root("stale");
    std::fs::write(directory.join("source.txt"), b"before").unwrap();
    let mut root = SourceRoot::open_directory(&directory).unwrap();
    let source = directory_source(&root, "source.txt");
    let request = StageRequest::new(
        SourceRootId::Directory {
            directory: root.directory().identity.clone(),
        },
        vec![SourceAction::Replace {
            source: source.clone(),
            expected: content(b"before"),
            edits: vec![edit(
                source,
                0,
                6,
                b"after",
                ActionProducer::unordered("fixture"),
            )],
        }],
    );
    std::fs::write(directory.join("source.txt"), b"changed").unwrap();

    assert_eq!(
        plan_mutations(&mut root, &request),
        Err(StageRefusal::Stale {
            inputs: vec![soopy::StaleInput {
                source: directory_source(&root, "source.txt"),
                expected: content(b"before"),
                observed: Some(content(b"changed")),
            }],
        })
    );
    assert_eq!(
        std::fs::read(directory.join("source.txt")).unwrap(),
        b"changed"
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn planner_accepts_adjacent_edits_and_merges_equivalent_producer_provenance() {
    let directory = temporary_root("adjacent");
    std::fs::write(directory.join("source.txt"), b"abcdef").unwrap();
    let mut root = SourceRoot::open_directory(&directory).unwrap();
    let source = directory_source(&root, "source.txt");
    let root_id = SourceRootId::Directory {
        directory: root.directory().identity.clone(),
    };
    let plan = plan_mutations(
        &mut root,
        &StageRequest::new(
            root_id,
            vec![SourceAction::Replace {
                source: source.clone(),
                expected: content(b"abcdef"),
                edits: vec![
                    edit(
                        source.clone(),
                        1,
                        3,
                        b"X",
                        ActionProducer::unordered("first"),
                    ),
                    edit(
                        source.clone(),
                        1,
                        3,
                        b"X",
                        ActionProducer::unordered("second"),
                    ),
                    edit(source, 3, 5, b"Y", ActionProducer::unordered("adjacent")),
                ],
            }],
        ),
    )
    .unwrap();

    assert_eq!(
        plan.files[0].bytes_after.as_deref(),
        Some(b"aXYf".as_slice())
    );
    assert_eq!(
        plan.files[0].edits[0].producers,
        vec![
            ActionProducer::unordered("first"),
            ActionProducer::unordered("second"),
        ]
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn planner_refuses_overlap_unordered_insertions_and_occupied_destinations() {
    let directory = temporary_root("conflicts");
    std::fs::write(directory.join("source.txt"), b"abcdef").unwrap();
    std::fs::write(directory.join("occupied.txt"), b"taken").unwrap();
    let mut root = SourceRoot::open_directory(&directory).unwrap();
    let root_id = SourceRootId::Directory {
        directory: root.directory().identity.clone(),
    };
    let source = directory_source(&root, "source.txt");

    let overlap = StageRequest::new(
        root_id.clone(),
        vec![SourceAction::Replace {
            source: source.clone(),
            expected: content(b"abcdef"),
            edits: vec![
                edit(
                    source.clone(),
                    1,
                    4,
                    b"first",
                    ActionProducer::unordered("first"),
                ),
                edit(
                    source.clone(),
                    3,
                    5,
                    b"second",
                    ActionProducer::unordered("second"),
                ),
            ],
        }],
    );
    assert!(matches!(
        plan_mutations(&mut root, &overlap),
        Err(StageRefusal::Conflict {
            conflicts,
        }) if matches!(conflicts.as_slice(), [MutationConflict::OverlappingEdits { .. }])
    ));

    let unordered = StageRequest::new(
        root_id.clone(),
        vec![SourceAction::Replace {
            source: source.clone(),
            expected: content(b"abcdef"),
            edits: vec![
                edit(
                    source.clone(),
                    0,
                    0,
                    b"first",
                    ActionProducer::unordered("first"),
                ),
                edit(
                    source.clone(),
                    0,
                    0,
                    b"second",
                    ActionProducer::unordered("second"),
                ),
            ],
        }],
    );
    assert!(matches!(
        plan_mutations(&mut root, &unordered),
        Err(StageRefusal::Conflict {
            conflicts,
        }) if matches!(conflicts.as_slice(), [MutationConflict::SameOffsetOrderRequired { .. }])
    ));

    let occupied = StageRequest::new(
        root_id,
        vec![SourceAction::Move {
            source,
            expected: content(b"abcdef"),
            destination: directory_path("occupied.txt"),
        }],
    );
    assert_eq!(
        plan_mutations(&mut root, &occupied),
        Err(StageRefusal::Conflict {
            conflicts: vec![MutationConflict::OccupiedDestination {
                path: directory_path("occupied.txt"),
            }],
        })
    );
    assert_eq!(
        std::fs::read(directory.join("source.txt")).unwrap(),
        b"abcdef"
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn planner_refusals_for_multiple_invalid_edits_and_expectations_are_input_order_independent() {
    let directory = temporary_root("deterministic_refusals");
    std::fs::write(directory.join("source.txt"), b"abc").unwrap();
    let mut root = SourceRoot::open_directory(&directory).unwrap();
    let root_id = SourceRootId::Directory {
        directory: root.directory().identity.clone(),
    };
    let source = directory_source(&root, "source.txt");
    let invalid_first = SourceAction::Replace {
        source: source.clone(),
        expected: content(b"abc"),
        edits: vec![
            edit(
                source.clone(),
                11,
                12,
                b"later",
                ActionProducer::unordered("later"),
            ),
            edit(
                source.clone(),
                8,
                9,
                b"first",
                ActionProducer::unordered("first"),
            ),
        ],
    };
    let mut invalid_reverse = invalid_first.clone();
    let SourceAction::Replace { edits, .. } = &mut invalid_reverse else {
        unreachable!("fixture action is replace");
    };
    edits.reverse();
    assert_eq!(
        plan_mutations(
            &mut root,
            &StageRequest::new(root_id.clone(), vec![invalid_first]),
        ),
        plan_mutations(
            &mut root,
            &StageRequest::new(root_id.clone(), vec![invalid_reverse]),
        )
    );

    let older = SourceAction::Replace {
        source: source.clone(),
        expected: content(b"older"),
        edits: Vec::new(),
    };
    let newer = SourceAction::Replace {
        source,
        expected: content(b"newer"),
        edits: Vec::new(),
    };
    assert_eq!(
        plan_mutations(
            &mut root,
            &StageRequest::new(root_id.clone(), vec![older.clone(), newer.clone()]),
        ),
        plan_mutations(&mut root, &StageRequest::new(root_id, vec![newer, older]))
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn planner_rejects_traversal_before_host_path_resolution() {
    let directory = temporary_root("traversal");
    let mut root = SourceRoot::open_directory(&directory).unwrap();
    let request = StageRequest::new(
        SourceRootId::Directory {
            directory: root.directory().identity.clone(),
        },
        vec![SourceAction::Create {
            path: directory_path("../outside.txt"),
            bytes: b"nope".to_vec(),
        }],
    );

    assert!(matches!(
        plan_mutations(&mut root, &request),
        Err(StageRefusal::InvalidPath { paths }) if paths[0].reason == soopy::InvalidPathReason::Traversal
    ));
    assert!(!directory.parent().unwrap().join("outside.txt").exists());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn planner_materializes_create_move_delete_and_refuses_destination_collisions() {
    let directory = temporary_root("path_actions");
    std::fs::write(directory.join("old.txt"), b"old").unwrap();
    std::fs::write(directory.join("gone.txt"), b"gone").unwrap();
    let mut root = SourceRoot::open_directory(&directory).unwrap();
    let root_id = SourceRootId::Directory {
        directory: root.directory().identity.clone(),
    };
    let old = directory_source(&root, "old.txt");
    let gone = directory_source(&root, "gone.txt");
    let plan = plan_mutations(
        &mut root,
        &StageRequest::new(
            root_id.clone(),
            vec![
                SourceAction::Create {
                    path: directory_path("created.txt"),
                    bytes: b"created".to_vec(),
                },
                SourceAction::Move {
                    source: old.clone(),
                    expected: content(b"old"),
                    destination: directory_path("moved.txt"),
                },
                SourceAction::Delete {
                    source: gone,
                    expected: content(b"gone"),
                },
            ],
        ),
    )
    .unwrap();

    assert_eq!(
        plan.files.iter().map(|file| file.kind).collect::<Vec<_>>(),
        vec![
            PlannedFileKind::Delete,
            PlannedFileKind::Create,
            PlannedFileKind::Move,
        ]
    );
    assert_eq!(plan.files[1].content_after, Some(content(b"created")));
    assert_eq!(plan.files[2].content_after, Some(content(b"old")));
    assert!(directory.join("old.txt").exists());
    assert!(!directory.join("moved.txt").exists());
    assert!(!directory.join("created.txt").exists());

    let collision = StageRequest::new(
        root_id,
        vec![
            SourceAction::Create {
                path: directory_path("target.txt"),
                bytes: b"first".to_vec(),
            },
            SourceAction::Move {
                source: old,
                expected: content(b"old"),
                destination: directory_path("target.txt"),
            },
        ],
    );
    assert!(matches!(
        plan_mutations(&mut root, &collision),
        Err(StageRefusal::Conflict { conflicts }) if conflicts.iter().any(|conflict| matches!(
            conflict,
            MutationConflict::PathCollision { path } if path == &directory_path("target.txt")
        ))
    ));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn git_and_directory_targets_materialize_equivalent_replacement_bytes() {
    let directory = temporary_root("git_equivalence");
    std::fs::write(directory.join("source.txt"), b"abcdef").unwrap();
    let mut plain = SourceRoot::open_directory(&directory).unwrap();
    let plain_source = directory_source(&plain, "source.txt");
    let plain_root = SourceRootId::Directory {
        directory: plain.directory().identity.clone(),
    };
    let plain_plan = plan_mutations(
        &mut plain,
        &StageRequest::new(
            plain_root,
            vec![SourceAction::Replace {
                source: plain_source.clone(),
                expected: content(b"abcdef"),
                edits: vec![edit(
                    plain_source,
                    1,
                    5,
                    b"X",
                    ActionProducer::unordered("fixture"),
                )],
            }],
        ),
    )
    .unwrap();

    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&directory)
        .status()
        .unwrap();
    let mut git = SourceRoot::discover_git(&directory).unwrap();
    let repository = git.git().unwrap().repository.clone();
    let revision = git
        .git_mut()
        .unwrap()
        .source_tree()
        .resolve_revision(soopy::Revision::Worktree)
        .unwrap();
    let git_source = ActionSource::Git {
        source: SourceRef {
            repository: repository.identity.clone(),
            revision,
            path: RepoPath(Arc::from("source.txt")),
        },
    };
    let git_status_before = std::process::Command::new("git")
        .args(["status", "--porcelain=v1"])
        .current_dir(&directory)
        .output()
        .unwrap();
    let git_plan = plan_mutations(
        &mut git,
        &StageRequest::new(
            SourceRootId::GitWorktree {
                repository: repository.identity,
                worktree: repository.worktree,
            },
            vec![SourceAction::Replace {
                source: git_source.clone(),
                expected: content(b"abcdef"),
                edits: vec![edit(
                    git_source,
                    1,
                    5,
                    b"X",
                    ActionProducer::unordered("fixture"),
                )],
            }],
        ),
    )
    .unwrap();

    assert_eq!(
        plain_plan.files[0].bytes_after,
        git_plan.files[0].bytes_after
    );
    assert_eq!(
        plain_plan.files[0].content_after,
        git_plan.files[0].content_after
    );
    assert_eq!(
        std::fs::read(directory.join("source.txt")).unwrap(),
        b"abcdef"
    );
    let git_status_after = std::process::Command::new("git")
        .args(["status", "--porcelain=v1"])
        .current_dir(&directory)
        .output()
        .unwrap();
    assert_eq!(git_status_after.stdout, git_status_before.stdout);
    std::fs::remove_dir_all(directory).unwrap();
}
