use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub struct Fixture {
    pub root: PathBuf,
    pub _directory: tempfile::TempDir,
}

impl Fixture {
    pub fn from_dir(name: &str) -> Self {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
        let mut files = Vec::new();
        let mut pending = vec![source.clone()];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else {
                    files.push((path.strip_prefix(&source).unwrap().to_str().unwrap().to_string(), std::fs::read_to_string(path).unwrap()));
                }
            }
        }
        Self::new(&files.iter().map(|(path, text)| (path.as_str(), text.as_str())).collect::<Vec<_>>())
    }

    pub fn new(files: &[(&str, &str)]) -> Self {
        let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plans/ryi-dogfood-validation");
        std::fs::create_dir_all(&scratch).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        for (path, text) in files {
            let target = root.join(path);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(target, text).unwrap();
        }
        let output = Command::new("git").args(["init", "-q"]).current_dir(&root).output().unwrap();
        assert!(output.status.success());
        Self { root: root.canonicalize().unwrap(), _directory: directory }
    }

    pub fn run(&self, directory: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ryii"))
            .current_dir(directory).args(args)
            .args(["--root", self.root.to_str().unwrap(), "--state"])
            .arg(self._directory.path().join("state"))
            .env("KACHE_DISABLED", "1").env("RUST_LOG", "off").output().unwrap()
    }

    pub fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.root.join(path)).unwrap()
    }
}
