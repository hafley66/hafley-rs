//! The worklist over the body provider: seeds first, then every project body an
//! edge reaches, breadth first. A thin caller of `WalkSession::body_edges`.

use super::*;
use std::collections::VecDeque;

use ra_ap_hir::DefWithBody;

use super::body_edges::{EdgeKind, WalkNode, WalkSession};

#[derive(Clone, Debug)]
pub struct WalkEdge {
    pub from: WalkNode,
    pub to: WalkNode,
    pub kind: EdgeKind,
    pub site_start: u32,
    pub site_end: u32,
    /// The popped body's depth plus one.
    pub depth: u32,
}

#[derive(Debug, Default)]
pub struct WalkAnswer {
    pub edges: Vec<WalkEdge>,
    /// Seed files the loaded crate graph declares no module for.
    pub unowned_seeds: Vec<String>,
    pub bodies_inferred: usize,
    pub load: Duration,
    pub walk: Duration,
}

/// One question: the seeds are `(supplied path, name)`, every body named
/// `name` declared in that file.
pub struct WalkQuestion<'q> {
    pub root: &'q Path,
    pub files: &'q [(String, PathBuf)],
    pub seeds: &'q [(String, String)],
    /// A body popped at this depth is not resolved.
    pub max_depth: Option<u32>,
    /// Runs from the first popped body (the load and def maps are not the walk);
    /// checked once per popped body.
    pub timeout: Option<Duration>,
    pub budget: Duration,
}

pub fn demand_walk(question: &WalkQuestion<'_>) -> Result<WalkAnswer, CheckerError> {
    let session = WalkSession::open(question.root, question.files, question.budget)?;
    let started = Instant::now();
    let _walk_span = crate::read::trace::tracked(tracing::info_span!("rust_walk")).entered();
    let mut seeds: Vec<&(String, String)> = question.seeds.iter().collect();
    seeds.sort();
    session.prime(&seeds.iter().map(|(path, _)| path.as_str()).collect::<Vec<_>>());
    let mut answer = WalkAnswer {
        load: session.load,
        ..WalkAnswer::default()
    };
    let mut seen: HashSet<DefWithBody> = HashSet::new();
    let mut queue: VecDeque<(DefWithBody, u32)> = VecDeque::new();
    for (path, name) in seeds {
        match session.seeds(path, name) {
            Some(bodies) => queue.extend(bodies.into_iter().filter(|body| seen.insert(*body)).map(|body| (body, 0))),
            None if !answer.unowned_seeds.contains(path) => answer.unowned_seeds.push(path.clone()),
            None => {}
        }
    }
    let deadline = question.timeout.map(|timeout| Instant::now() + timeout);
    while let Some((body, depth)) = queue.pop_front() {
        if deadline.is_some_and(|at| Instant::now() >= at) {
            return Err(CheckerError::Deadline);
        }
        if question.max_depth.is_some_and(|cap| depth >= cap) {
            continue;
        }
        let Some(out) = session.body_edges(body) else {
            continue;
        };
        answer.bodies_inferred += 1;
        for edge in out.edges {
            if let Some(next) = edge.to_body.filter(|next| seen.insert(*next)) {
                queue.push_back((next, depth + 1));
            }
            answer.edges.push(WalkEdge {
                from: out.from.clone(),
                to: edge.to,
                kind: edge.kind,
                site_start: edge.site_start,
                site_end: edge.site_end,
                depth: depth + 1,
            });
        }
    }
    answer.walk = started.elapsed();
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch crate outside every Cargo workspace.
    fn scratch(lib: &str) -> (tempfile::TempDir, PathBuf, Vec<(String, PathBuf)>) {
        let dir = tempfile::TempDir::new().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
        )
        .unwrap();
        std::fs::write(root.join("src/lib.rs"), lib).unwrap();
        let files = vec![("src/lib.rs".to_string(), root.join("src/lib.rs"))];
        (dir, root, files)
    }

    #[test]
    fn walk_reaches_calls_methods_passed_items_and_trait_impls_and_stops_at_extern() {
        let (_dir, root, files) = scratch(
            "#[derive(PartialEq, Eq, PartialOrd)]\n\
             pub struct S;\n\
             impl S { pub fn m(&self) -> u8 { helper() } }\n\
             impl Ord for S {\n\
                 fn cmp(&self, _: &Self) -> std::cmp::Ordering { compared(); std::cmp::Ordering::Equal }\n\
             }\n\
             fn helper() -> u8 { 1 }\n\
             fn compared() {}\n\
             fn mapped(x: u8) -> u8 { x }\n\
             fn unreached() { helper(); }\n\
             pub fn seed() {\n\
                 let s = S;\n\
                 s.m();\n\
                 let _ = Some(1u8).map(mapped);\n\
                 let _ = std::cmp::max(S, S);\n\
                 let _ = format!(\"{}\", helper());\n\
             }\n",
        );
        let seeds = [("src/lib.rs".to_string(), "seed".to_string())];
        let question = WalkQuestion {
            root: &root,
            files: &files,
            seeds: &seeds,
            max_depth: None,
            timeout: None,
            budget: Duration::from_secs(120),
        };
        let answer = demand_walk(&question).unwrap();
        let mut edges: Vec<String> = answer
            .edges
            .iter()
            .map(|edge| {
                let to = if edge.to.path.is_empty() { format!("extern {}", edge.to.name) } else { edge.to.name.clone() };
                format!("{} -{:?}-> {} @{}", edge.from.name, edge.kind, to, edge.depth)
            })
            .collect();
        edges.sort();
        assert_eq!(
            edges,
            [
                "cmp -Call-> compared @2",
                "m -Call-> helper @2",
                "seed -Call-> extern core::cmp::max @1",
                "seed -Call-> extern core::option::Some @1",
                "seed -Method-> extern core::option::Option::map @1",
                "seed -Method-> m @1",
                "seed -Passed-> mapped @1",
                "seed -TraitImpl-> cmp @1",
                "seed -TraitImpl-> cmp @1",
            ]
        );
        // seed, m, mapped, cmp, helper, compared: `unreached` and the `format!`
        // argument (an unexpanded macro, as in the eager walk) are never read.
        assert_eq!(answer.bodies_inferred, 6);
    }

    #[test]
    fn a_passed_deadline_stops_the_walk() {
        let (_dir, root, files) = scratch("pub fn seed() {}\n");
        let seeds = [("src/lib.rs".to_string(), "seed".to_string())];
        let question = WalkQuestion {
            root: &root,
            files: &files,
            seeds: &seeds,
            max_depth: None,
            timeout: Some(Duration::ZERO),
            budget: Duration::from_secs(120),
        };
        assert!(matches!(demand_walk(&question), Err(CheckerError::Deadline)));
    }
}
