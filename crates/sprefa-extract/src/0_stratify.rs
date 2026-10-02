//! `ryi stratify`: dependency-first file strata and locality proposals.

use crate::cli::StratifyArgs;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::Write;
use std::path::{Path, PathBuf};

use sprefa_extract::{
    resolve_project, FlatFact, ResolveArms, ResolveRequest, ScipMode, ScipRecords,
};

#[derive(Clone, Debug)]
struct Edge {
    from: String,
    from_name: Option<String>,
    to: String,
    to_name: Option<String>,
    to_start: Option<u32>,
}

#[derive(Clone, Debug)]
struct Seed {
    path: String,
    name: Option<String>,
    rank: Option<i64>,
    via: String,
}

#[derive(Clone, Debug)]
struct Reach {
    depth: u32,
    via: String,
}

pub fn run(args: StratifyArgs) -> Result<(), crate::RyiExit> {
    run_to(args, &mut std::io::stdout().lock())
}

pub fn run_to(args: StratifyArgs, out: &mut dyn Write) -> Result<(), crate::RyiExit> {
    let root = crate::inputs::root(&args.inputs);
    let mut inputs = args.inputs.clone();
    if inputs.paths.is_empty() && inputs.entry.is_empty() {
        inputs.paths.push(root.clone());
    }
    let paths = crate::inputs::expand(&inputs)
        .map_err(|error| crate::RyiExit::new(2, format!("ryi stratify: {error}")))?;
    if paths.is_empty() {
        return Err(crate::RyiExit::new(2, "ryi stratify: no source files"));
    }
    let arms = match args.kind.as_str() {
        "call" => ResolveArms {
            call: true,
            ..ResolveArms::default()
        },
        "type" => ResolveArms {
            types: true,
            ..ResolveArms::default()
        },
        "both" => ResolveArms {
            call: true,
            types: true,
            ..ResolveArms::default()
        },
        other => {
            return Err(crate::RyiExit::new(
                2,
                format!("ryi stratify: unknown --kind {other}; use call, type, or both"),
            ));
        }
    };
    let request = ResolveRequest {
        paths: &paths,
        arms,
        scip: ScipMode::Off,
        project_root: args.inputs.root.as_deref(),
        scip_records: ScipRecords::default(),
        occurrence_text: false,
        rust_checker: None,
        ts_checker: None,
        go_checker: None,
        witness: false,
    };
    let facts = resolve_project(&request)
        .map_err(|error| crate::RyiExit::new(1, format!("ryi stratify: {error}")))?;
    let mut edges = Vec::new();
    let mut file_set = BTreeSet::new();
    for path in &paths {
        file_set.insert(path.to_string_lossy().replace('\\', "/"));
    }
    for fact in facts {
        match fact {
            FlatFact::ResolvedEdge {
                caller_path,
                caller_name,
                callee_path,
                callee_name,
                callee_start,
                ..
            } => {
                file_set.insert(caller_path.clone());
                file_set.insert(callee_path.clone());
                edges.push(Edge {
                    from: caller_path,
                    from_name: caller_name,
                    to: callee_path,
                    to_name: callee_name,
                    to_start: Some(callee_start),
                });
            }
            FlatFact::ResolvedTypeEdge {
                owner_path,
                owner_name,
                target_path,
                target_name,
                ..
            } => {
                file_set.insert(owner_path.clone());
                file_set.insert(target_path.clone());
                edges.push(Edge {
                    from: owner_path,
                    from_name: owner_name,
                    to: target_path,
                    to_name: target_name,
                    to_start: None,
                });
            }
            FlatFact::ResolvedImportRow {
                src_path,
                target_path,
                name,
                ..
            } => {
                file_set.insert(src_path.clone());
                file_set.insert(target_path.clone());
                edges.push(Edge {
                    from: src_path,
                    from_name: Some(name),
                    to: target_path,
                    to_name: None,
                    to_start: None,
                });
            }
            _ => {}
        }
    }
    edges.sort_by_key(|edge| {
        (
            edge.from.clone(),
            edge.from_name.clone(),
            edge.to.clone(),
            edge.to_name.clone(),
            edge.to_start,
        )
    });
    let files: Vec<String> = file_set.into_iter().collect();
    let seeds = parse_seeds(&args.from, &root)?;
    let ranks = reachable_depths(&files, &edges, &seeds, &args.kind);
    let components = strongly_connected(&files, &edges);
    let reached: Vec<String> = ranks
        .iter()
        .filter_map(|(path, reach)| reach.as_ref().map(|_| path.clone()))
        .collect();
    let reached_components = strongly_connected(&reached, &edges);
    let (depths, max_depth) = component_depths(&reached_components, &edges);
    let line_counts = files
        .iter()
        .map(|path| (path.clone(), line_count(&root, path)))
        .collect::<BTreeMap<_, _>>();
    let median = args
        .base_lines
        .unwrap_or_else(|| median(line_counts.values().copied()));
    let locality = localities(&files, &edges, &line_counts, median);
    let mut rows = Vec::new();
    for (path, reach) in &ranks {
        if let Some(reach) = reach {
            if is_index(path) {
                rows.push(serde_json::json!({"record":"stratum","depth":depths[&reached_components[path]],"entry_depth":reach.depth,"path":path,"prefix":"","scc":components[path],"via":reach.via}));
                continue;
            }
            let digits = if max_depth > 9 { 2 } else { 1 };
            let prefix = format!(
                "{:0width$}_",
                depths[&reached_components[path]],
                width = digits
            );
            let destination = prefixed(path, &prefix);
            rows.push(serde_json::json!({"record":"stratum","depth":depths[&reached_components[path]],"entry_depth":reach.depth,"path":path,"prefix":prefix,"scc":components[path],"via":reach.via}));
            rows.push(serde_json::json!({"record":"stratify_move","from_path":path,"to_path":destination,"reason":"depth_prefix","move_tsv":format!("{path}\t{destination}")}));
        } else {
            rows.push(serde_json::json!({"record":"unreached","path":path,"scc":components[path]}));
        }
    }
    let mut cycles: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for path in &files {
        cycles
            .entry(components[path])
            .or_default()
            .push(path.clone());
    }
    for (scc, members) in cycles {
        if members.len() > 1 {
            rows.push(serde_json::json!({"record":"stratum_cycle","scc":scc,"members":members.len(),"paths":members}));
        }
    }
    for (path, internal, touching, score, lines, target) in locality {
        rows.push(serde_json::json!({"record":"locality","path":path,"internal":internal,"touching":touching,"score":score,"lines":lines,"target_lines":target}));
        let outgoing = edges
            .iter()
            .filter(|edge| edge.from == path && edge.to != path)
            .collect::<Vec<_>>();
        let incoming = edges
            .iter()
            .filter(|edge| edge.to == path && edge.from != path)
            .count();
        let reason = if touching == 0 {
            None
        } else if score >= 0.5 && lines.saturating_mul(2) < target {
            components
                .iter()
                .find(|(candidate, scc)| **candidate != path && **scc == components[&path])
                .map(|(candidate, _)| ("merge_candidate", candidate.clone()))
        } else if score < 0.35 && lines > target {
            Some(("split_candidate", path.clone()))
        } else if score < 0.35 && lines <= median {
            best_neighbor(&outgoing).map(|path| ("move_candidate", path))
        } else {
            None
        };
        if let Some((reason, to_path)) = reason {
            let cut_lines = if reason == "split_candidate" {
                symbol_cut_lines(&edges, &path, &root)
            } else {
                Vec::new()
            };
            let shared_edges = edges
                .iter()
                .filter(|edge| {
                    (edge.from == path && edge.to == to_path)
                        || (edge.from == to_path && edge.to == path)
                })
                .count();
            rows.push(serde_json::json!({"record":"stratify_move","from_path":path,"to_path":to_path,"reason":reason,"cut_lines":cut_lines,"shared_edges":shared_edges.max(incoming)}));
        }
    }
    rows.sort_by_key(|row| {
        (
            row.get("record")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string(),
            row.get("path")
                .or_else(|| row.get("from_path"))
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string(),
        )
    });
    for row in rows {
        writeln!(out, "{row}").map_err(|error| crate::RyiExit::new(1, error.to_string()))?;
    }
    Ok(())
}

fn parse_seeds(values: &[String], root: &Path) -> Result<Vec<Seed>, crate::RyiExit> {
    if values.is_empty() {
        return Err(crate::RyiExit::new(
            2,
            "ryi stratify: pass at least one --from PATH[:NAME][=RANK]",
        ));
    }
    values
        .iter()
        .map(|value| {
            let (base, rank) = match value.rsplit_once('=') {
                Some((base, rank)) => (
                    base,
                    Some(rank.parse::<i64>().map_err(|_| {
                        crate::RyiExit::new(2, format!("invalid entrypoint rank in {value}"))
                    })?),
                ),
                None => (value.as_str(), None),
            };
            let (path, name) = match base.rsplit_once(':') {
                Some((path, name)) if !path.is_empty() && !name.is_empty() => {
                    (path, Some(name.to_string()))
                }
                _ => (base, None),
            };
            let path = PathBuf::from(path);
            let path = if path.is_absolute() {
                path.strip_prefix(root).unwrap_or(&path).to_path_buf()
            } else {
                path
            };
            let path = path.to_string_lossy().replace('\\', "/");
            let via = match &name {
                Some(name) => format!("{path}:{name}"),
                None => path.clone(),
            };
            Ok(Seed {
                path,
                name,
                rank,
                via,
            })
        })
        .collect()
}

fn reachable_depths(
    files: &[String],
    edges: &[Edge],
    seeds: &[Seed],
    kind: &str,
) -> BTreeMap<String, Option<Reach>> {
    let mut ranked: BTreeMap<String, (i64, u32, String)> = BTreeMap::new();
    let mut unranked: BTreeMap<String, (u32, String)> = BTreeMap::new();
    for seed in seeds {
        let mut queue = VecDeque::from([(seed.path.clone(), seed.name.clone(), 0u32)]);
        let mut seen = BTreeSet::new();
        while let Some((path, name, depth)) = queue.pop_front() {
            let node = (path.clone(), name.clone());
            if !seen.insert(node) {
                continue;
            }
            if seed.rank.is_some() {
                let rank = seed.rank.unwrap();
                ranked
                    .entry(path.clone())
                    .and_modify(|current| {
                        if rank > current.0 || (rank == current.0 && depth < current.1) {
                            *current = (rank, depth, seed.via.clone());
                        }
                    })
                    .or_insert((rank, depth, seed.via.clone()));
            } else {
                unranked
                    .entry(path.clone())
                    .and_modify(|current| {
                        if depth < current.0 {
                            *current = (depth, seed.via.clone());
                        }
                    })
                    .or_insert((depth, seed.via.clone()));
            }
            for edge in edges
                .iter()
                .filter(|edge| edge.from == path && (kind != "call" || edge.from_name.is_some()))
            {
                if name
                    .as_ref()
                    .is_some_and(|current| edge.from_name.as_ref() != Some(current))
                {
                    continue;
                }
                queue.push_back((
                    edge.to.clone(),
                    edge.to_name.clone(),
                    depth.saturating_add(1),
                ));
            }
            if name.is_none() {
                for edge in edges.iter().filter(|edge| edge.from == path) {
                    queue.push_back((
                        edge.to.clone(),
                        edge.to_name.clone(),
                        depth.saturating_add(1),
                    ));
                }
            }
        }
    }
    files
        .iter()
        .map(|path| {
            let reach = ranked
                .get(path)
                .map(|(_, depth, via)| Reach {
                    depth: *depth,
                    via: via.clone(),
                })
                .or_else(|| {
                    unranked.get(path).map(|(depth, via)| Reach {
                        depth: *depth,
                        via: via.clone(),
                    })
                });
            (path.clone(), reach)
        })
        .collect()
}

fn strongly_connected(files: &[String], edges: &[Edge]) -> BTreeMap<String, usize> {
    fn visit(
        node: &str,
        graph: &BTreeMap<String, BTreeSet<String>>,
        seen: &mut BTreeSet<String>,
        order: &mut Vec<String>,
    ) {
        if !seen.insert(node.to_string()) {
            return;
        }
        if let Some(next) = graph.get(node) {
            for child in next {
                visit(child, graph, seen, order);
            }
        }
        order.push(node.to_string());
    }
    let mut graph: BTreeMap<String, BTreeSet<String>> = files
        .iter()
        .map(|path| (path.clone(), BTreeSet::new()))
        .collect();
    let mut reverse = graph.clone();
    for edge in edges {
        graph
            .entry(edge.from.clone())
            .or_default()
            .insert(edge.to.clone());
        reverse
            .entry(edge.to.clone())
            .or_default()
            .insert(edge.from.clone());
    }
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    for path in files {
        visit(path, &graph, &mut seen, &mut order);
    }
    seen.clear();
    let mut members = Vec::new();
    for path in order.into_iter().rev() {
        if seen.contains(&path) {
            continue;
        }
        let mut component = Vec::new();
        visit(&path, &reverse, &mut seen, &mut component);
        component.sort();
        members.push(component);
    }
    members.sort_by_key(|component| component[0].clone());
    members
        .into_iter()
        .enumerate()
        .flat_map(|(id, paths)| paths.into_iter().map(move |path| (path, id)))
        .collect()
}

fn component_depths(
    components: &BTreeMap<String, usize>,
    edges: &[Edge],
) -> (BTreeMap<usize, u32>, u32) {
    let mut dependencies: BTreeMap<usize, BTreeSet<usize>> = components
        .values()
        .map(|id| (*id, BTreeSet::new()))
        .collect();
    for edge in edges {
        let (Some(from), Some(to)) = (components.get(&edge.from), components.get(&edge.to)) else {
            continue;
        };
        if from != to {
            dependencies.entry(*from).or_default().insert(*to);
        }
    }
    let mut remaining: BTreeMap<usize, usize> = dependencies
        .iter()
        .map(|(id, deps)| (*id, deps.len()))
        .collect();
    let mut reverse: BTreeMap<usize, BTreeSet<usize>> =
        remaining.keys().map(|id| (*id, BTreeSet::new())).collect();
    for (from, deps) in &dependencies {
        for dep in deps {
            reverse.entry(*dep).or_default().insert(*from);
        }
    }
    let mut queue: BTreeSet<usize> = remaining
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(*id))
        .collect();
    let mut depths: BTreeMap<usize, u32> = BTreeMap::new();
    while let Some(id) = queue.pop_first() {
        let depth = dependencies[&id]
            .iter()
            .map(|dep| depths[dep].saturating_add(1))
            .max()
            .unwrap_or(0);
        depths.insert(id, depth);
        for parent in &reverse[&id] {
            let count = remaining.get_mut(parent).unwrap();
            *count -= 1;
            if *count == 0 {
                queue.insert(*parent);
            }
        }
    }
    let max_depth = depths.values().copied().max().unwrap_or(0);
    (depths, max_depth)
}

fn line_count(root: &Path, path: &str) -> u32 {
    let path = Path::new(path);
    let content = std::fs::read(path).or_else(|_| std::fs::read(root.join(path)));
    content
        .map(|bytes| {
            bytes
                .iter()
                .filter(|byte| **byte == b'\n')
                .count()
                .saturating_add(1) as u32
        })
        .unwrap_or(0)
}

fn median(values: impl Iterator<Item = u32>) -> u32 {
    let mut values: Vec<_> = values.collect();
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    values[(values.len() - 1) / 2]
}

fn localities(
    files: &[String],
    edges: &[Edge],
    lines: &BTreeMap<String, u32>,
    base: u32,
) -> Vec<(String, u32, u32, f32, u32, u32)> {
    files
        .iter()
        .map(|path| {
            let touching = edges
                .iter()
                .filter(|edge| edge.from == *path || edge.to == *path)
                .count() as u32;
            let internal = edges
                .iter()
                .filter(|edge| edge.from == *path && edge.to == *path)
                .count() as u32;
            let score = if touching == 0 {
                0.0
            } else {
                internal as f32 / touching as f32
            };
            let count = lines.get(path).copied().unwrap_or(0);
            let target = (base as f32 * score).round() as u32;
            (path.clone(), internal, touching, score, count, target)
        })
        .collect()
}

fn best_neighbor(outgoing: &[&Edge]) -> Option<String> {
    let mut counts = BTreeMap::<&str, usize>::new();
    for edge in outgoing {
        *counts.entry(&edge.to).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(left.0)))
        .map(|(path, _)| path.to_string())
}

fn is_index(path: &str) -> bool {
    matches!(
        Path::new(path).file_name().and_then(|name| name.to_str()),
        Some("index.ts" | "index.tsx" | "mod.rs" | "lib.rs")
    )
}

fn prefixed(path: &str, prefix: &str) -> String {
    let path = Path::new(path);
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let stem = name.split_once('_').and_then(|(number, stem)| {
        let digits = number.bytes().take_while(u8::is_ascii_digit).count();
        (digits > 0 && number[digits..].bytes().all(|byte| byte.is_ascii_alphabetic()))
            .then_some(stem)
    }).unwrap_or(&name);
    parent
        .join(format!("{prefix}{stem}"))
        .to_string_lossy()
        .replace('\\', "/")
}

fn symbol_cut_lines(edges: &[Edge], path: &str, root: &Path) -> Vec<u32> {
    let symbols: Vec<Edge> = edges
        .iter()
        .filter(|edge| edge.from == path && edge.to == path)
        .filter_map(|edge| {
            Some(Edge {
                from: edge.from_name.clone()?,
                from_name: None,
                to: edge.to_name.clone()?,
                to_name: None,
                to_start: edge.to_start,
            })
        })
        .collect();
    let names: BTreeSet<String> = symbols
        .iter()
        .flat_map(|edge| [edge.from.clone(), edge.to.clone()])
        .collect();
    let names: Vec<String> = names.into_iter().collect();
    let components = strongly_connected(&names, &symbols);
    let bytes = std::fs::read(Path::new(path))
        .or_else(|_| std::fs::read(root.join(path)))
        .ok();
    let Some(bytes) = bytes else {
        return Vec::new();
    };
    symbols
        .iter()
        .filter(|edge| components.get(&edge.from) != components.get(&edge.to))
        .filter_map(|edge| edge.to_start)
        .filter_map(|offset| {
            let end = (offset as usize).min(bytes.len());
            Some(bytes[..end].iter().filter(|byte| **byte == b'\n').count() as u32 + 1)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
