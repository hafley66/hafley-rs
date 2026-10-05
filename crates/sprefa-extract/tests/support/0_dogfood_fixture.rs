use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub struct Fixture {
    pub root: PathBuf,
    pub _directory: tempfile::TempDir,
}

impl Fixture {
    pub fn new(files: &[(&str, &str)]) -> Self {
        let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plans/ryi-dogfood-validation");
        std::fs::create_dir_all(&scratch).unwrap();
        let directory = tempfile::tempdir_in(scratch).unwrap();
        let root = directory.path().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        for (path, text) in files {
            let target = root.join(path);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(target, text).unwrap();
        }
        let output = Command::new("git").args(["init", "-q"]).current_dir(&root).output().unwrap();
        assert!(output.status.success());
        Self { root, _directory: directory }
    }

    pub fn run(&self, directory: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ryii"))
            .current_dir(directory).args(args)
            .args(["--root", self.root.to_str().unwrap(), "--state"])
            .arg(self._directory.path().join("state"))
            .env("KACHE_DISABLED", "1").output().unwrap()
    }

    pub fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.root.join(path)).unwrap()
    }
}
