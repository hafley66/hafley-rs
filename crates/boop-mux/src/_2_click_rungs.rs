//! The rungs of the ⌘-click ladder: pure token shape, directory joins, the
//! gitignore-aware index and its rankings, sibling checkouts, and git history.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use ignore::WalkBuilder;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

pub(crate) const INDEX_TTL: Duration = Duration::from_secs(30);
pub(crate) const INDEX_CAP: usize = 20_000;
pub(crate) const MAX_RUNGS: usize = 8;
pub(crate) const MAX_SIBLINGS: usize = 400;
pub(crate) const MAX_GIT_REPOS: usize = 4;
pub(crate) const MAX_GIT_REVS: usize = 20;
pub const MAX_CHOICES: usize = 50;

// nucleo pays 16 per matched char plus boundary bonuses; under 12 per char the
// match is a coincidental subsequence and the token belongs to ripgrep.
pub(crate) const MIN_SCORE_PER_CHAR: u32 = 12;
// Shorter queries match most of an index.
pub(crate) const MIN_FUZZY_QUERY: usize = 4;

#[derive(Clone, Debug)]
pub struct IndexEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
}

/// Split a trailing line reference off a token: `:123`, a span `:123-145`,
/// or a list `:561,583`. The line is the first number. Drive letters and
/// `host:8080` in a URL are not line refs.
pub fn split_line_ref(token: &str) -> (String, Option<u32>) {
    let Some(colon) = token.rfind(':') else {
        return (token.to_string(), None);
    };
    let (head, tail) = token.split_at(colon);
    let numbers: Vec<&str> = tail[1..].split(['-', ',']).map(str::trim).collect();
    let spans = tail[1..].matches('-').count();
    let is_line = numbers.iter().all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        && (spans == 0 || (spans == 1 && numbers.len() == 2));
    // `C:\src` drive letters and `http://host:8080` are not line references.
    let is_url_port = head.split("://").nth(1).is_some_and(|rest| !rest.contains('/'));
    if !is_line || head.is_empty() || head.len() == 1 || is_url_port {
        return (token.to_string(), None);
    }
    match numbers[0].parse::<u32>() {
        Ok(line) => (head.to_string(), Some(line)),
        Err(_) => (token.to_string(), None),
    }
}

/// A token worth walking the filesystem for: it carries a separator or an
/// extension-looking tail. Bare words are symbols far more often than paths.
pub fn looks_like_path(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("www.") {
        return false;
    }
    if token.contains('/') || token.contains('~') {
        return true;
    }
    match token.rfind('.') {
        Some(dot) if dot + 1 < token.len() => {
            let ext = &token[dot + 1..];
            ext.len() <= 16 && ext.bytes().all(|b| b.is_ascii_alphanumeric())
        }
        _ => false,
    }
}

pub(crate) fn trim_slash(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        "/"
    } else {
        trimmed
    }
}

/// Directories from `cwd` up to and including `boundary`, nearest first. A cwd
/// outside the boundary walks toward the root instead.
pub fn ancestors_of(cwd: &str, boundary: &str, max: usize) -> Vec<String> {
    let start = trim_slash(cwd);
    if start.is_empty() || start == "/" {
        return Vec::new();
    }
    let stop = if boundary.is_empty() { "/" } else { trim_slash(boundary) };
    let inside = start == stop || start.starts_with(&format!("{stop}/"));
    let mut out = Vec::new();
    let mut dir = start.to_string();
    while out.len() < max {
        out.push(dir.clone());
        if inside && dir == stop {
            break;
        }
        let Some(cut) = dir.rfind('/') else { break };
        let parent = if cut == 0 { "/" } else { &dir[..cut] };
        if parent == dir || parent == "/" {
            break;
        }
        dir = parent.to_string();
    }
    out
}

/// Full paths to stat, best guess first, tagged with the rung that produced them.
pub fn crawl_candidates(
    rel: &str,
    cwd: &str,
    repo_root: Option<&str>,
    boundary: &str,
    max: usize,
) -> Vec<(String, &'static str)> {
    if rel.starts_with('/') || rel.starts_with("~/") {
        return vec![(rel.to_string(), "absolute")];
    }
    let tail = rel.strip_prefix("./").unwrap_or(rel);
    let mut out: Vec<(String, &'static str)> = Vec::new();
    let push = |dir: &str, step: &'static str, out: &mut Vec<(String, &'static str)>| {
        if dir.is_empty() {
            return;
        }
        let path = format!("{}/{}", trim_slash(dir), tail);
        if out.iter().any(|(seen, _)| seen == &path) {
            return;
        }
        out.push((path, step));
    };
    push(cwd, "cwd", &mut out);
    if let Some(root) = repo_root {
        push(root, "repo", &mut out);
    }
    for dir in ancestors_of(cwd, boundary, max) {
        push(&dir, "ancestor", &mut out);
    }
    if out.is_empty() {
        out.push((tail.to_string(), "cwd"));
    }
    out
}

/// The repository (or worktree) a directory belongs to: the nearest ancestor
/// holding a `.git` entry, file or directory.
pub fn repo_root_for(cwd: &str) -> Option<String> {
    let mut dir = Path::new(cwd);
    loop {
        if dir.join(".git").exists() {
            return Some(dir.to_string_lossy().into_owned());
        }
        dir = dir.parent()?;
    }
}

/// The nearest Git root for a file or directory path: a file walks from its
/// parent directory.
pub fn repo_root_of(path: &str) -> Option<String> {
    let candidate = Path::new(path);
    let directory = if candidate.is_dir() {
        candidate
    } else {
        candidate.parent().unwrap_or_else(|| Path::new("."))
    };
    repo_root_for(&directory.to_string_lossy())
}

/// Search hits ranked the way an exact tail match deserves: a path ending with
/// the whole token beats one that only shares a name, shallower beats deeper.
/// Directories are hits: agents print folder paths as often as file paths.
pub fn rank_exact(rel: &str, entries: &[IndexEntry]) -> Vec<(String, bool)> {
    let tail = rel.trim_start_matches("./").trim_start_matches('/');
    let base = tail.rsplit('/').next().unwrap_or(tail);
    let mut scored: Vec<(u8, usize, &str)> = entries
        .iter()
        .filter_map(|e| {
            let suffix = e.path.ends_with(&format!("/{tail}")) || e.path == tail;
            let named = e.name == base;
            if !suffix && !named {
                return None;
            }
            Some((
                if suffix { 0 } else { 1 },
                e.path.matches('/').count(),
                e.path.as_str(),
            ))
        })
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(b.2)));
    scored.into_iter().map(|(rank, _, path)| (path.to_string(), rank == 0)).collect()
}

/// `rel` joined to each indexed directory, kept when it exists on disk. This is
/// the one rung that reaches into gitignored folders: their parent is indexed
/// even when their contents are not. Shallowest match first.
pub fn under_indexed_dirs(rel: &str, entries: &[IndexEntry]) -> Vec<String> {
    let tail = rel.trim_start_matches("./").trim_start_matches('/');
    if tail.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<String> = entries
        .iter()
        .filter(|e| e.is_dir)
        .filter_map(|e| {
            let candidate = Path::new(&e.path).join(tail);
            std::fs::symlink_metadata(&candidate)
                .is_ok()
                .then(|| candidate.to_string_lossy().into_owned())
        })
        .collect();
    found.sort_by(|a, b| a.matches('/').count().cmp(&b.matches('/').count()).then(a.cmp(b)));
    found.dedup();
    found
}

/// fzf ranking over the index. A token carrying a separator matches whole paths;
/// a bare filename matches basenames, so a folder chain cannot out-score a file.
pub fn rank_fuzzy(query: &str, entries: &[IndexEntry], limit: usize) -> Vec<(String, u32)> {
    let clean = query.trim().trim_start_matches("./").trim_end_matches('/');
    if clean.chars().count() < MIN_FUZZY_QUERY || entries.is_empty() {
        return Vec::new();
    }
    let scoped = clean.contains('/');
    let mut config = Config::DEFAULT;
    config.set_match_paths();
    let mut matcher = Matcher::new(config);
    let pattern = Pattern::parse(clean, CaseMatching::Ignore, Normalization::Smart);
    // `by-test.md` may fuzz its stem, never its extension: a candidate that is
    // not a `.md` file is a coincidence of letters, not the file.
    let extension: Option<String> = clean
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .filter(|(stem, ext)| !stem.is_empty() && !ext.is_empty() && ext.len() <= 16 && ext.chars().all(|c| c.is_ascii_alphanumeric()))
        .map(|(_, ext)| format!(".{}", ext.to_ascii_lowercase()));
    let haystack: Vec<&str> = entries
        .iter()
        .map(|e| {
            let keep = extension
                .as_deref()
                .is_none_or(|ext| e.name.to_ascii_lowercase().ends_with(ext));
            if !keep {
                ""
            } else if scoped {
                e.path.as_str()
            } else {
                e.name.as_str()
            }
        })
        .collect();
    let floor = MIN_SCORE_PER_CHAR * clean.chars().filter(|c| *c != '/').count().min(64) as u32;
    let mut buf = Vec::new();
    let mut hits: Vec<(usize, u32)> = Vec::new();
    for (index, candidate) in haystack.iter().enumerate() {
        if candidate.is_empty() {
            continue;
        }
        let Some(score) = pattern.score(Utf32Str::new(candidate, &mut buf), &mut matcher) else {
            continue;
        };
        if score >= floor {
            hits.push((index, score));
        }
    }
    // fzf's own tiebreakers: score, then the shorter candidate, then the name.
    hits.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then(entries[a.0].path.matches('/').count().cmp(&entries[b.0].path.matches('/').count()))
            .then(entries[a.0].path.len().cmp(&entries[b.0].path.len()))
            .then(entries[a.0].path.cmp(&entries[b.0].path))
    });
    hits.truncate(limit);
    hits.into_iter()
        .map(|(index, score)| (entries[index].path.clone(), score))
        .collect()
}

/// Exactly one directory named `token`, or nothing: a bare word that matches
/// several folders (or none) is a ripgrep query.
pub fn unique_dir_named(token: &str, entries: &[IndexEntry]) -> Option<String> {
    let want = token.trim().to_ascii_lowercase();
    if want.chars().count() < MIN_FUZZY_QUERY {
        return None;
    }
    let mut found: Option<&IndexEntry> = None;
    for entry in entries.iter().filter(|e| e.is_dir) {
        if !entry.name.eq_ignore_ascii_case(&want) {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some(entry);
    }
    found.map(|e| e.path.clone())
}

/// Sibling checkouts: agent output prints a path relative to ITS repo root, so a
/// token that misses every ancestor join is tried under the neighbouring repos.
/// Only directories holding a `.git` count, and only at or above the repo root.
pub fn sibling_candidates(
    rel: &str,
    cwd: &str,
    repo_root: Option<&str>,
    boundary: &str,
    max: usize,
) -> Vec<String> {
    let mut out = Vec::new();
    let start = repo_root.unwrap_or(cwd);
    for ancestor in ancestors_of(start, boundary, max) {
        let Ok(children) = std::fs::read_dir(&ancestor) else { continue };
        let mut seen = 0;
        for child in children.flatten() {
            seen += 1;
            if seen > MAX_SIBLINGS {
                break;
            }
            if !child.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let path = child.path();
            if child.file_name().to_string_lossy().starts_with('.') || !path.join(".git").exists() {
                continue;
            }
            out.push(format!("{}/{}", path.to_string_lossy(), rel));
        }
    }
    out
}

/// Repos worth asking git about: a checkout whose first path segment exists, so
/// `plans/x.md` only questions repos that actually have a `plans` directory.
pub(crate) fn git_probe_repos(rel: &str, cwd: &str, repo_root: Option<&str>, boundary: &str) -> Vec<String> {
    let head = rel.split('/').next().unwrap_or(rel);
    let mut repos: Vec<String> = repo_root.map(|r| vec![r.to_string()]).unwrap_or_default();
    for ancestor in ancestors_of(cwd, boundary, MAX_RUNGS) {
        let Ok(children) = std::fs::read_dir(&ancestor) else { continue };
        for child in children.flatten().take(MAX_SIBLINGS) {
            let path = child.path();
            if !path.join(".git").exists() || !path.join(head).is_dir() {
                continue;
            }
            let repo = path.to_string_lossy().into_owned();
            if !repos.contains(&repo) {
                repos.push(repo);
            }
            if repos.len() >= MAX_GIT_REPOS {
                return repos;
            }
        }
    }
    repos
}

pub fn git_out(repo: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git").arg("-C").arg(repo).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// A path git knows and the working tree does not: deleted, on another branch,
/// or fetched but never checked out. Answers with the newest commit holding it.
pub fn git_absent(rel: &str, repos: &[String]) -> Option<(String, String, String)> {
    for repo in repos {
        let Some(revs) = git_out(repo, &["rev-list", "--all", "--", rel]) else { continue };
        // The newest commit touching a path can be the one that deleted it, so
        // walk back until a revision still holds the blob.
        for rev in revs.lines().take(MAX_GIT_REVS) {
            if git_out(repo, &["cat-file", "-e", &format!("{rev}:{rel}")]).is_none() {
                continue;
            }
            let subject = git_out(repo, &["log", "-1", "--format=%h %s", rev]).unwrap_or_default();
            return Some((repo.clone(), rev.to_string(), subject));
        }
    }
    None
}

type IndexCache = Mutex<HashMap<PathBuf, (Instant, Arc<Vec<IndexEntry>>)>>;

fn index_cache() -> &'static IndexCache {
    static CACHE: OnceLock<IndexCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The gitignore-aware file and directory list under `root`, cached briefly: a
/// wall of agent output resolves many tokens against the same tree.
pub(crate) fn index_for(root: &Path) -> Arc<Vec<IndexEntry>> {
    let key = root.to_path_buf();
    if let Some((at, entries)) = index_cache().lock().ok().and_then(|c| c.get(&key).cloned()) {
        if at.elapsed() < INDEX_TTL {
            return entries;
        }
    }
    let mut entries = Vec::new();
    let mut walker = WalkBuilder::new(root);
    walker
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true)
        .follow_links(false)
        // Dependency trees and git internals are never what a pasted path
        // means; keeping them out stops fzf offering node_modules noise.
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            name != "node_modules" && name != ".git"
        });
    for result in walker.build() {
        if entries.len() >= INDEX_CAP {
            break;
        }
        let Ok(entry) = result else { continue };
        let Some(file_type) = entry.file_type() else { continue };
        let path = entry.path();
        if path == root {
            continue;
        }
        entries.push(IndexEntry {
            path: path.to_string_lossy().into_owned(),
            name: entry.file_name().to_string_lossy().into_owned(),
            is_dir: file_type.is_dir(),
        });
    }
    let entries = Arc::new(entries);
    if let Ok(mut cache) = index_cache().lock() {
        cache.insert(key, (Instant::now(), entries.clone()));
    }
    entries
}

/// Drop the cached indexes (a preview watch reporting a change calls this).
pub fn clear_index_cache() {
    if let Ok(mut cache) = index_cache().lock() {
        cache.clear();
    }
}
pub fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_default()
}

/// Whether an absolute (`/…`) or home-anchored (`~/…`) token names something on
/// disk. The token may be several paths a soft join stitched together, so the
/// rung that answers with the token itself has to look. An empty HOME leaves
/// `~/…` trusted, since only the renderer expands it before opening.
pub(crate) fn absolute_on_disk(rel: &str, home: &str) -> bool {
    match rel.strip_prefix("~/") {
        Some(tail) => !home.is_empty() && std::fs::symlink_metadata(Path::new(home).join(tail)).is_ok(),
        None => std::fs::symlink_metadata(rel).is_ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const HOME: &str = "/Users/me";
    const REPO: &str = "/Users/me/projects/instant";

    #[test]
    fn repo_root_accepts_a_file_path() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().join("repo");
        let docs = root.join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(root.join(".git"), "gitdir: test").unwrap();
        let file = docs.join("guide.md");

        assert_eq!(
            repo_root_of(&file.to_string_lossy()),
            Some(root.to_string_lossy().into_owned())
        );
    }

    fn file(path: &str) -> IndexEntry {
        IndexEntry {
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            is_dir: false,
        }
    }

    fn dir(path: &str) -> IndexEntry {
        IndexEntry { is_dir: true, ..file(path) }
    }

    fn index() -> Vec<IndexEntry> {
        vec![
            file(&format!("{REPO}/src/main.ts")),
            file(&format!("{REPO}/src/preview.ts")),
            file(&format!("{REPO}/src/mdview/MdPanel.tsx")),
            file(&format!("{REPO}/e2e/MdPanel.tsx")),
            file(&format!("{REPO}/packages/patchset-diff/src/index.ts")),
            dir(&format!("{REPO}/src")),
            dir(&format!("{REPO}/src/mdview")),
            dir(&format!("{REPO}/e2e")),
            dir(&format!("{REPO}/packages")),
            dir(&format!("{REPO}/packages/patchset-diff")),
        ]
    }

    /// RECEIPT. A repo-relative token whose file sits inside a gitignored
    /// directory resolves to that file, before fzf gets to guess.
    /// RECEIPT. A token with an extension never fuzzes onto a file of another
    /// kind: `out/by-test.md` finds no `.md` here, so fzf answers nothing.
    #[test]
    fn fzf_keeps_the_extension_literal() {
        let entries = vec![
            file(&format!("{REPO}/node_modules/.pnpm/buffer-equal-constant-time@1.0.1/node_modules/buffer-equal-constant-time/index.js")),
            dir(&format!("{REPO}/node_modules/.pnpm/buffer-equal-constant-time@1.0.1/node_modules/buffer-equal-constant-time")),
            file(&format!("{REPO}/lab/out/by-tests.md")),
        ];
        let hits = rank_fuzzy("out/by-test.md", &entries, 20);
        assert_eq!(hits.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(), vec![format!("{REPO}/lab/out/by-tests.md").as_str()]);
        assert!(rank_fuzzy("out/by-test.txt", &entries, 20).is_empty());
    }

    #[test]
    fn splits_a_line_reference() {
        assert_eq!(split_line_ref("src/main.ts:214"), ("src/main.ts".into(), Some(214)));
        assert_eq!(split_line_ref("main.ts"), ("main.ts".into(), None));
        assert_eq!(split_line_ref("http://host:8080"), ("http://host:8080".into(), None));
        assert_eq!(split_line_ref("C:8"), ("C:8".into(), None));
    }

    /// RECEIPT. Markdown cites a span or a list of lines; the file opens at
    /// the first.
    #[test]
    fn splits_a_line_range_and_a_line_list() {
        let forms = [
            "src/lang/rust/2_call.rs:790-801",
            "2_call.rs:183-198",
            "rust_modules.rs:1105-1136",
            "2_call.rs:561,583",
            "2_call.rs:561, 583",
            "hafley_scm/src/lang/rust/10_module_resolution_rows.rs:123-145",
            "a.rs:7-",
            "a.rs:-7",
            "a.rs:7,,8",
            "http://host:80-81",
        ];
        let split: Vec<(String, Option<u32>)> = forms.iter().map(|form| split_line_ref(form)).collect();
        assert_eq!(
            split,
            vec![
                ("src/lang/rust/2_call.rs".into(), Some(790)),
                ("2_call.rs".into(), Some(183)),
                ("rust_modules.rs".into(), Some(1105)),
                ("2_call.rs".into(), Some(561)),
                ("2_call.rs".into(), Some(561)),
                ("hafley_scm/src/lang/rust/10_module_resolution_rows.rs".into(), Some(123)),
                ("a.rs:7-".into(), None),
                ("a.rs:-7".into(), None),
                ("a.rs:7,,8".into(), None),
                ("http://host:80-81".into(), None),
            ]
        );
    }

    #[test]
    fn recognizes_path_shapes() {
        assert!(looks_like_path("src/main.ts"));
        assert!(looks_like_path("MdPanel.tsx"));
        assert!(looks_like_path("~/notes.md"));
        assert!(!looks_like_path("renderPathInto"));
        assert!(!looks_like_path("https://example.com/a.ts"));
    }

    #[test]
    fn walks_ancestors_to_the_boundary() {
        assert_eq!(
            ancestors_of(&format!("{REPO}/src/mdview"), HOME, 8),
            vec![
                format!("{REPO}/src/mdview"),
                format!("{REPO}/src"),
                REPO.to_string(),
                format!("{HOME}/projects"),
                HOME.to_string(),
            ]
        );
        assert_eq!(ancestors_of(&format!("{REPO}/src"), HOME, 1), vec![format!("{REPO}/src")]);
        assert_eq!(ancestors_of("/tmp/e2e/src", HOME, 8), vec!["/tmp/e2e/src", "/tmp/e2e", "/tmp"]);
        assert!(ancestors_of("/", HOME, 8).is_empty());
    }

    #[test]
    fn orders_the_rungs_cwd_then_repo_then_ancestors() {
        let candidates = crawl_candidates("notes.md", &format!("{REPO}/src"), Some(REPO), HOME, 8);
        assert_eq!(
            candidates,
            vec![
                (format!("{REPO}/src/notes.md"), "cwd"),
                (format!("{REPO}/notes.md"), "repo"),
                (format!("{HOME}/projects/notes.md"), "ancestor"),
                (format!("{HOME}/notes.md"), "ancestor"),
            ]
        );
    }

    #[test]
    fn reaches_a_sibling_repo_and_leaves_absolutes_alone() {
        let paths: Vec<String> =
            crawl_candidates("instant-lanes/README.md", &format!("{REPO}/src"), Some(REPO), HOME, 8)
                .into_iter()
                .map(|(path, _)| path)
                .collect();
        assert!(paths.contains(&format!("{HOME}/projects/instant-lanes/README.md")));
        assert_eq!(
            crawl_candidates("/a/b.ts", &format!("{REPO}/src"), Some(REPO), HOME, 8),
            vec![("/a/b.ts".to_string(), "absolute")]
        );
    }

    #[test]
    fn ranks_an_exact_tail_over_a_filename() {
        let entries = index();
        assert_eq!(
            rank_exact("MdPanel.tsx", &entries),
            vec![
                (format!("{REPO}/e2e/MdPanel.tsx"), true),
                (format!("{REPO}/src/mdview/MdPanel.tsx"), true),
            ]
        );
        // The whole-tail match sorts first and is the only one marked as such.
        assert_eq!(
            rank_exact("mdview/MdPanel.tsx", &entries),
            vec![
                (format!("{REPO}/src/mdview/MdPanel.tsx"), true),
                (format!("{REPO}/e2e/MdPanel.tsx"), false),
            ]
        );
        assert!(rank_exact("nope.ts", &entries).is_empty());
    }

    /// RECEIPT. A folder path is an exact hit, not a fuzzy afterthought.
    #[test]
    fn an_exact_tail_names_a_directory() {
        assert_eq!(
            rank_exact("packages/patchset-diff", &index()),
            vec![(format!("{REPO}/packages/patchset-diff"), true)]
        );
    }

    #[test]
    fn fzf_finds_abbreviated_names_and_folders() {
        let entries = index();
        assert_eq!(rank_fuzzy("prevew.ts", &entries, 5)[0].0, format!("{REPO}/src/preview.ts"));
        let mdpanel: Vec<String> = rank_fuzzy("mdpanel", &entries, 5).into_iter().map(|(p, _)| p).collect();
        assert_eq!(
            mdpanel,
            vec![format!("{REPO}/e2e/MdPanel.tsx"), format!("{REPO}/src/mdview/MdPanel.tsx")]
        );
        assert_eq!(
            rank_fuzzy("patchset-diff", &entries, 5)[0].0,
            format!("{REPO}/packages/patchset-diff")
        );
    }

    #[test]
    fn fzf_refuses_noise() {
        let entries = index();
        assert!(rank_fuzzy("qqqqzz", &entries, 5).is_empty());
        assert!(rank_fuzzy("nope.ts", &entries, 5).is_empty());
        assert!(rank_fuzzy(".ts", &entries, 5).is_empty());
    }

    #[test]
    fn folder_words_resolve_only_when_unique() {
        let entries = index();
        assert_eq!(
            unique_dir_named("patchset-diff", &entries),
            Some(format!("{REPO}/packages/patchset-diff"))
        );
        assert_eq!(unique_dir_named("mdview", &entries), Some(format!("{REPO}/src/mdview")));
        assert_eq!(unique_dir_named("e2e", &entries), None);
        assert_eq!(unique_dir_named("MdPanel.tsx", &entries), None);
    }
}
