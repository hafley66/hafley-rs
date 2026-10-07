use std::path::{Path, PathBuf};

pub struct Build<'a> {
    pub hash: &'a str,
    pub date: &'a str,
    pub root: &'a Path,
}

fn git_directory(root: &Path) -> Option<PathBuf> {
    let dot_git = root.join(".git");
    let directory = if dot_git.is_dir() {
        dot_git
    } else {
        let file = std::fs::read_to_string(dot_git).ok()?;
        root.join(file.trim().strip_prefix("gitdir: ")?)
    };
    match std::fs::read_to_string(directory.join("commondir")) {
        Ok(common) => Some(directory.join(common.trim())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(directory),
        Err(_) => None,
    }
}

/// origin/main when the checkout tracks a remote, else the local main branch.
pub fn main_hash(root: &Path) -> Option<String> {
    let directory = git_directory(root)?;
    let packed = std::fs::read_to_string(directory.join("packed-refs")).unwrap_or_default();
    ["refs/remotes/origin/main", "refs/heads/main"].into_iter().find_map(|reference| {
        let loose = std::fs::read_to_string(directory.join(reference)).ok()
            .map(|hash| hash.trim().to_owned())
            .filter(|hash| !hash.is_empty());
        loose.or_else(|| packed.lines().find_map(|line| {
            let (hash, name) = line.split_once(' ')?;
            (name == reference).then(|| hash.to_owned())
        }))
    })
}

impl Build<'_> {
    pub fn warning(&self, binary: &str, enabled: bool) -> String {
        if !enabled { return String::new(); }
        let Some(main) = main_hash(self.root) else { return String::new(); };
        if main == self.hash { return String::new(); }
        let crate_path = match binary { "ryii" => "crates/sprefa-extract", _ => "crates/ryi" };
        let features = if binary == "ryii" { " --features cli,read" } else { "" };
        let path = self.root.join(crate_path).display().to_string().replace('\'', "'\\''");
        format!("{binary}: stale build {} ({}) from {}; main is {main}. Rebuild: cargo install --locked --path '{path}'{features} --bin {binary} --force\n", self.hash, self.date, self.root.display())
    }

    pub fn version(&self, binary: &str) -> String {
        let verdict = match main_hash(self.root) {
            Some(main) if main == self.hash => "current".to_owned(),
            Some(main) => format!("stale (main {main})"),
            None => "unknown (main unreadable)".to_owned(),
        };
        format!("{binary} {}\ngit hash: {}\nbuild date: {}\nrepo root: {}\nstale verdict: {verdict}\n", env!("CARGO_PKG_VERSION"), self.hash, self.date, self.root.display())
    }
}

pub fn startup(binary: &str) -> bool {
    let build = Build {
        hash: env!("SPREFA_BUILD_GIT_HASH"),
        date: env!("SPREFA_BUILD_DATETIME"),
        root: Path::new(env!("SPREFA_BUILD_REPO_ROOT")),
    };
    eprint!("{}", build.warning(binary, std::env::var("RYI_STALE_CHECK").as_deref() == Ok("1")));
    let version = std::env::args_os().len() == 2 && std::env::args_os().nth(1)
        .is_some_and(|arg| arg == "--version" || arg == "-V");
    if version { print!("{}", build.version(binary)); }
    version
}
