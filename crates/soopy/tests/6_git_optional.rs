use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use soopy::{
    ContentId, DirectoryDelta, DirectoryRoot, FileQuery, FileReadRequest, FileWatchQuery, Pattern,
    SourceRoot, WatchCoalescing,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
const NO_GIT_CHILD: &str = "SOOPY_NO_GIT_CHILD";
const NO_GIT_ROOT: &str = "SOOPY_NO_GIT_ROOT";

fn directory(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "soopy_git_optional_{}_{}_{}_{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed),
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn contains_delta(deltas: &[DirectoryDelta], expected: &DirectoryDelta) -> bool {
    deltas.iter().any(|delta| delta == expected)
}

fn wait_for(
    watcher: &mut soopy::DirectoryWatcher,
    expected: DirectoryDelta,
) -> Vec<DirectoryDelta> {
    for _ in 0..5 {
        let Some(deltas) = watcher.recv_timeout(Duration::from_secs(1)).unwrap() else {
            continue;
        };
        if contains_delta(&deltas, &expected) {
            return deltas;
        }
    }
    panic!("directory watcher did not emit {expected:?}");
}

#[test]
fn plain_directory_snapshot_and_streaming_read_have_no_git_coordinates() {
    let root = directory("plain");
    std::fs::create_dir_all(root.join("nested")).unwrap();
    std::fs::write(root.join("nested/file.txt"), b"first\n").unwrap();
    let mut directory = DirectoryRoot::open(&root).unwrap();
    let snapshot = directory.snapshot(&FileQuery::default()).unwrap();
    assert_eq!(snapshot.files.len(), 1);
    assert_eq!(snapshot.files[0].file.path.0.as_ref(), "nested/file.txt");
    assert_eq!(snapshot.directories.len(), 1);
    assert_eq!(snapshot.directories[0].0.as_ref(), "nested");
    assert!(matches!(snapshot.files[0].content, ContentId::Blake3(_)));

    let request = FileReadRequest {
        file: snapshot.files[0].file.clone(),
        expected: Some(snapshot.files[0].content.clone()),
    };
    directory
        .read_each(&[request], |bytes| {
            assert_eq!(bytes.bytes, b"first\n");
            assert!(matches!(bytes.content, ContentId::Blake3(_)));
            Ok(())
        })
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn open_directory_does_not_require_git_on_path() {
    if std::env::var_os(NO_GIT_CHILD).is_some() {
        let root = std::env::var_os(NO_GIT_ROOT).unwrap();
        assert!(matches!(
            SourceRoot::open_directory(root).unwrap(),
            SourceRoot::Directory(_)
        ));
        return;
    }

    let root = directory("no_git_path");
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "open_directory_does_not_require_git_on_path"])
        .env(NO_GIT_CHILD, "1")
        .env(NO_GIT_ROOT, &root)
        .env("PATH", "")
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn plain_directory_watcher_reports_add_change_and_remove() {
    let root = directory("watch");
    let directory = DirectoryRoot::open(&root).unwrap();
    let mut watcher = directory
        .watch(FileWatchQuery {
            patterns: vec![Pattern("**/*.txt".into())],
            coalescing: WatchCoalescing {
                quiet_ms: 30,
                max_ms: 250,
            },
        })
        .unwrap();

    let file = root.join("watched.txt");
    std::fs::write(&file, b"one\n").unwrap();
    wait_for(
        &mut watcher,
        DirectoryDelta::Added(PathBuf::from("watched.txt")),
    );
    std::fs::write(&file, b"two\n").unwrap();
    wait_for(
        &mut watcher,
        DirectoryDelta::Changed(PathBuf::from("watched.txt")),
    );
    std::fs::remove_file(&file).unwrap();
    wait_for(
        &mut watcher,
        DirectoryDelta::Removed(PathBuf::from("watched.txt")),
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn directory_mode_excludes_git_metadata_but_keeps_nested_repository_files() {
    let root = directory("nested");
    std::fs::create_dir_all(root.join("inner/src")).unwrap();
    let output = Command::new("git")
        .arg("-C")
        .arg(root.join("inner"))
        .args(["init", "-q"])
        .output()
        .unwrap();
    assert!(output.status.success());
    std::fs::write(root.join("inner/src/file.txt"), b"nested\n").unwrap();
    let mut directory = DirectoryRoot::open(&root).unwrap();
    let snapshot = directory.snapshot(&FileQuery::default()).unwrap();
    let paths = snapshot
        .files
        .iter()
        .map(|entry| entry.file.path.0.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(paths, vec!["inner/src/file.txt"]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn source_root_requires_explicit_git_discovery() {
    let root = directory("discover");
    assert!(matches!(
        SourceRoot::open_directory(&root).unwrap(),
        SourceRoot::Directory(_)
    ));
    let output = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["init", "-q"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let mut discovered = SourceRoot::discover_git(&root).unwrap();
    let git = discovered.git().unwrap();
    assert_eq!(git.directory.root, git.repository.root);
    assert!(discovered
        .git_mut()
        .unwrap()
        .source_tree()
        .repository()
        .root
        .exists());
    std::fs::remove_dir_all(root).unwrap();
}
