//! Node identity is (path, name), never the bare name: two structs sharing a
//! name in two files are two nodes, which is what the wire's split paths say.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use sprefa_extract::{
    dispatch, flatten, resolve_project, FamilyMask, FamilyTag, FlatFact, ResolveArms,
    ResolveRequest, ScipMode,
};

/// At most this many shapes on one board. Over budget splits, never crams.
const SHAPE_BUDGET: usize = 24;
const BOARD_NODE_BUDGET: usize = SHAPE_BUDGET - 1;
const MARKDOWN_START: &str = "<!-- ryi:typegraph-d2:start -->";
const MARKDOWN_END: &str = "<!-- ryi:typegraph-d2:end -->";

/// A node key. `path` is as the resolve was invoked with it.
type Key = (String, String);

#[derive(Clone, Debug)]
struct Scc {
    members: Vec<Key>,
    topo_layer: usize,
    cyclic: bool,
}

struct TypeGraph {
    hop_distance: BTreeMap<Key, usize>,
    components: Vec<Scc>,
    component_of: BTreeMap<Key, usize>,
    edges: Vec<TypeEdge>,
}

struct Board {
    topo_layer: usize,
    nodes: Vec<Key>,
}

struct Args {
    root: PathBuf,
    entry: String,
    out: PathBuf,
    markdown_into: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut root = None;
    let mut entry = None;
    let mut out = None;
    let mut markdown_into = None;
    let mut argv = std::env::args().skip(1);
    while let Some(flag) = argv.next() {
        match flag.as_str() {
            "--root" => root = Some(PathBuf::from(argv_next(&mut argv, &flag)?)),
            "--entry" => entry = Some(argv_next(&mut argv, &flag)?),
            "--out" => out = Some(PathBuf::from(argv_next(&mut argv, &flag)?)),
            "--markdown-into" => markdown_into = Some(PathBuf::from(argv_next(&mut argv, &flag)?)),
            other => return Err(format!("unknown flag {other}")),
        }
    }
    Ok(Args {
        root: root.ok_or("--root DIR is required")?,
        entry: entry.ok_or("--entry PATH::NAME is required")?,
        out: out.ok_or("--out DIR is required")?,
        markdown_into,
    })
}

fn argv_next(argv: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    argv.next().ok_or_else(|| format!("{flag} takes a value"))
}

/// Every `.rs` under `root`, `target/` excluded, sorted.
fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name == "target") {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// (path, name) -> entity kind, from the phase-1 type nodes of each file.
fn node_kinds(paths: &[PathBuf]) -> BTreeMap<Key, String> {
    let mut kinds = BTreeMap::new();
    for path in paths {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let key = path.to_string_lossy().to_string();
        let Some(output) = dispatch(&key, &bytes, FamilyMask::ALL) else {
            continue;
        };
        for fact in flatten(&output) {
            if let FlatFact::Node {
                family: FamilyTag::Type,
                kind,
                name: Some(name),
                ..
            } = fact
            {
                kinds.entry((key.clone(), name)).or_insert(kind);
            }
        }
    }
    kinds
}

/// One resolved type reference, owner to target.
#[derive(Clone, Debug)]
struct TypeEdge {
    src: Key,
    dst: Key,
    kind: String,
}

fn type_edges(paths: &[PathBuf]) -> Result<Vec<TypeEdge>, String> {
    let request = ResolveRequest {
        paths,
        arms: ResolveArms {
            call: false,
            types: true,
            flow: false,
        },
        scip: ScipMode::Off,
        project_root: None,
        scip_records: Default::default(),
        occurrence_text: false,
        rust_checker: None,
        ts_checker: None,
        go_checker: None,
        witness: false,
    };
    let facts = resolve_project(&request).map_err(|err| err.to_string())?;
    Ok(facts
        .into_iter()
        .filter_map(|fact| match fact {
            FlatFact::ResolvedTypeEdge {
                owner_path,
                owner_name: Some(owner_name),
                target_path,
                target_name: Some(target_name),
                kind,
                ..
            } => Some(TypeEdge {
                src: (owner_path, owner_name),
                dst: (target_path, target_name),
                kind,
            }),
            _ => None,
        })
        .collect())
}

/// Reachable type nodes keep their BFS distance while SCC condensation assigns
/// a separate topological layer for board placement.
fn reachable_graph(entry: &Key, edges: &[TypeEdge]) -> TypeGraph {
    let mut out_edges: BTreeMap<Key, BTreeSet<Key>> = BTreeMap::new();
    for edge in edges {
        out_edges
            .entry(edge.src.clone())
            .or_default()
            .insert(edge.dst.clone());
    }

    let mut hop_distance = BTreeMap::from([(entry.clone(), 0usize)]);
    let mut queue = VecDeque::from([entry.clone()]);
    while let Some(current) = queue.pop_front() {
        let next_hop = hop_distance[&current] + 1;
        for next in out_edges.get(&current).into_iter().flatten() {
            if !hop_distance.contains_key(next) {
                hop_distance.insert(next.clone(), next_hop);
                queue.push_back(next.clone());
            }
        }
    }

    let mut reachable_edges: Vec<TypeEdge> = edges
        .iter()
        .filter(|edge| hop_distance.contains_key(&edge.src) && hop_distance.contains_key(&edge.dst))
        .cloned()
        .collect();
    reachable_edges.sort_by(|left, right| {
        (&left.src, &left.dst, &left.kind).cmp(&(&right.src, &right.dst, &right.kind))
    });

    let nodes: Vec<Key> = hop_distance.keys().cloned().collect();
    let mut adjacency: BTreeMap<Key, BTreeSet<Key>> = nodes
        .iter()
        .cloned()
        .map(|key| (key, BTreeSet::new()))
        .collect();
    let mut reverse: BTreeMap<Key, BTreeSet<Key>> = nodes
        .iter()
        .cloned()
        .map(|key| (key, BTreeSet::new()))
        .collect();
    for edge in &reachable_edges {
        adjacency
            .get_mut(&edge.src)
            .expect("reachable source has an adjacency row")
            .insert(edge.dst.clone());
        reverse
            .get_mut(&edge.dst)
            .expect("reachable target has a reverse adjacency row")
            .insert(edge.src.clone());
    }
    let adjacency: BTreeMap<Key, Vec<Key>> = adjacency
        .into_iter()
        .map(|(key, neighbors)| (key, neighbors.into_iter().collect()))
        .collect();

    // Kosaraju's two passes use ordered adjacency and ordered roots, so SCC
    // discovery never depends on hash iteration.
    let mut visited = BTreeSet::new();
    let mut finished = Vec::with_capacity(nodes.len());
    for root in &nodes {
        if !visited.insert(root.clone()) {
            continue;
        }
        let mut stack = vec![(root.clone(), 0usize)];
        while !stack.is_empty() {
            let next = {
                let (current, next_index) = stack.last_mut().expect("nonempty DFS stack");
                let neighbors = adjacency.get(current).expect("reachable adjacency row");
                if *next_index < neighbors.len() {
                    let next = neighbors[*next_index].clone();
                    *next_index += 1;
                    Some(next)
                } else {
                    None
                }
            };
            if let Some(next) = next {
                if visited.insert(next.clone()) {
                    stack.push((next, 0));
                }
            } else {
                finished.push(stack.pop().expect("nonempty DFS stack").0);
            }
        }
    }

    let mut assigned = BTreeSet::new();
    let mut members = Vec::new();
    for root in finished.into_iter().rev() {
        if !assigned.insert(root.clone()) {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![root];
        while let Some(current) = stack.pop() {
            component.push(current.clone());
            for previous in reverse
                .get(&current)
                .expect("reachable reverse adjacency row")
                .iter()
                .rev()
            {
                if assigned.insert(previous.clone()) {
                    stack.push(previous.clone());
                }
            }
        }
        component.sort();
        members.push(component);
    }
    members.sort_by(|left, right| left[0].cmp(&right[0]));

    let mut component_of = BTreeMap::new();
    for (component, keys) in members.iter().enumerate() {
        for key in keys {
            component_of.insert(key.clone(), component);
        }
    }
    let mut component_edges = vec![BTreeSet::new(); members.len()];
    let mut indegree = vec![0usize; members.len()];
    for edge in &reachable_edges {
        let from = component_of[&edge.src];
        let to = component_of[&edge.dst];
        if from != to && component_edges[from].insert(to) {
            indegree[to] += 1;
        }
    }

    let mut ready = BTreeSet::new();
    for (component, degree) in indegree.iter().enumerate() {
        if *degree == 0 {
            ready.insert((members[component][0].clone(), component));
        }
    }
    let mut topo_layers = vec![0usize; members.len()];
    let mut visited_components = 0usize;
    while let Some((_, from)) = ready.pop_first() {
        visited_components += 1;
        for to in &component_edges[from] {
            topo_layers[*to] = topo_layers[*to].max(topo_layers[from] + 1);
            indegree[*to] -= 1;
            if indegree[*to] == 0 {
                ready.insert((members[*to][0].clone(), *to));
            }
        }
    }
    debug_assert_eq!(
        visited_components,
        members.len(),
        "SCC condensation is a DAG"
    );

    let components = members
        .into_iter()
        .enumerate()
        .map(|(index, keys)| Scc {
            cyclic: keys.len() > 1 || adjacency[&keys[0]].contains(&keys[0]),
            members: keys,
            topo_layer: topo_layers[index],
        })
        .collect();
    TypeGraph {
        hop_distance,
        components,
        component_of,
        edges: reachable_edges,
    }
}

/// A d2 identifier: everything outside `[A-Za-z0-9_]` becomes `_`.
fn ident(key: &Key) -> String {
    let raw = format!("{}__{}", key.0, key.1);
    raw.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

/// d2 label text: the bare name, truncated so no shape carries a paragraph.
fn label(key: &Key) -> String {
    let name = &key.1;
    if name.chars().count() <= 40 {
        return name.clone();
    }
    let head: String = name.chars().take(39).collect();
    format!("{head}…")
}

/// Fill per `TypeEntityKind` slug. Unknown kinds take the last row.
fn fill_for(kind: &str) -> &'static str {
    match kind {
        "struct" | "class" => "#1f4e79",
        "enum" => "#7b3f00",
        "trait" | "interface" => "#4b2e83",
        "alias" => "#3f5f3f",
        "function" | "method" => "#5a5a2d",
        _ => "#5a3a5a",
    }
}

/// `direction: down` is what makes the board wider than tall. Each source edge
/// is emitted on its source board; a deterministic boundary stub and edge
/// label identify the remote target and destination board at a board boundary.
fn board(
    board: &Board,
    board_index: usize,
    graph: &TypeGraph,
    kinds: &BTreeMap<Key, String>,
    node_board: &BTreeMap<Key, usize>,
) -> String {
    let mut text = String::from("direction: down\n\n");
    for key in &board.nodes {
        let kind = kinds.get(key).map(String::as_str).unwrap_or("struct");
        let component_index = graph.component_of[key];
        let component = &graph.components[component_index];
        let hop = graph.hop_distance[key];
        let mut node_label = format!("{} (hop {hop}", label(key));
        if component.cyclic {
            node_label.push_str(&format!(", SCC {} cycle", component_index + 1));
        }
        node_label.push(')');
        text.push_str(&format!(
            "{}: {} {{ shape: rectangle; style.fill: \"{}\"; style.font-color: \"#ffffff\" }}\n",
            ident(key),
            node_label,
            fill_for(kind)
        ));
    }
    text.push('\n');
    let present: BTreeSet<&Key> = board.nodes.iter().collect();
    let has_boundary = graph
        .edges
        .iter()
        .any(|edge| present.contains(&edge.src) && node_board[&edge.dst] != board_index);
    let boundary_id = format!("ryi_boundary_stub_{}", board_index + 1);
    if has_boundary {
        text.push_str(&format!(
            "{}: boundary stub {{ shape: rectangle; style.fill: \"#59636e\"; style.font-color: \"#ffffff\" }}\n",
            boundary_id,
        ));
    }
    if has_boundary {
        text.push('\n');
    }
    for edge in &graph.edges {
        if present.contains(&edge.src) {
            let (dst, edge_label) = if node_board[&edge.dst] == board_index {
                (ident(&edge.dst), edge.kind.clone())
            } else {
                (
                    boundary_id.clone(),
                    format!(
                        "{} to {} at {} (board {})",
                        edge.kind,
                        edge.dst.1,
                        edge.dst.0,
                        node_board[&edge.dst] + 1
                    ),
                )
            };
            text.push_str(&format!(
                "{} -> {}: {}\n",
                ident(&edge.src),
                dst,
                edge_label
            ));
        }
    }
    text
}

fn push_board(out: &mut Vec<Board>, topo_layer: usize, nodes: &mut Vec<Key>) {
    if !nodes.is_empty() {
        out.push(Board {
            topo_layer,
            nodes: std::mem::take(nodes),
        });
    }
}

/// Group whole SCCs by their DAG layer, reserving one shape for a possible
/// boundary stub. An oversized SCC is chunked by stable node order.
fn boards(graph: &TypeGraph) -> Vec<Board> {
    let mut by_layer: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (component, scc) in graph.components.iter().enumerate() {
        by_layer.entry(scc.topo_layer).or_default().push(component);
    }
    let mut out = Vec::new();
    for (topo_layer, components) in by_layer {
        let mut current = Vec::new();
        for component in components {
            let members = &graph.components[component].members;
            if members.len() > BOARD_NODE_BUDGET {
                push_board(&mut out, topo_layer, &mut current);
                for chunk in members.chunks(BOARD_NODE_BUDGET) {
                    out.push(Board {
                        topo_layer,
                        nodes: chunk.to_vec(),
                    });
                }
            } else {
                if current.len() + members.len() > BOARD_NODE_BUDGET {
                    push_board(&mut out, topo_layer, &mut current);
                }
                current.extend(members.iter().cloned());
            }
        }
        push_board(&mut out, topo_layer, &mut current);
    }
    out
}

/// Replace only the marked block, keeping the surrounding document unchanged.
fn regenerate_markdown(path: &Path, entry: &str, boards: &[(usize, String)]) -> Result<(), String> {
    let original = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
    let start = original
        .find(MARKDOWN_START)
        .ok_or_else(|| format!("{} has no {MARKDOWN_START}", path.display()))?;
    let body_start = start + MARKDOWN_START.len();
    let end = original[body_start..]
        .find(MARKDOWN_END)
        .map(|offset| body_start + offset)
        .ok_or_else(|| format!("{} has no {MARKDOWN_END}", path.display()))?;
    if original[body_start..].contains(MARKDOWN_START)
        || original[end + MARKDOWN_END.len()..].contains(MARKDOWN_END)
    {
        return Err(format!(
            "{} has duplicate typegraph markers",
            path.display()
        ));
    }
    let mut generated = String::new();
    for (index, (topo_layer, board)) in boards.iter().enumerate() {
        generated.push_str(&format!(
            "<details>\n<summary>Ryi type graph: {entry}, board {}/{}, topological layer {topo_layer}</summary>\n\n```d2\n{board}```\n\n</details>\n\n",
            index + 1,
            boards.len()
        ));
    }
    let updated = format!(
        "{}\n\n{}{}",
        &original[..body_start],
        generated,
        &original[end..]
    );
    std::fs::write(path, updated).map_err(|err| err.to_string())
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let (entry_path, entry_name) = args
        .entry
        .rsplit_once("::")
        .ok_or("--entry must be PATH::NAME")?;
    let entry: Key = (entry_path.to_string(), entry_name.to_string());

    let paths = rust_files(&args.root);
    if paths.is_empty() {
        return Err(format!("no .rs files under {}", args.root.display()));
    }
    let kinds = node_kinds(&paths);
    if !kinds.contains_key(&entry) {
        let near: Vec<&String> = kinds
            .keys()
            .filter(|(_, name)| name == &entry.1)
            .map(|(path, _)| path)
            .take(5)
            .collect();
        return Err(format!(
            "no type node {}::{} ; the same name appears in: {near:?}",
            entry.0, entry.1
        ));
    }
    let edges = type_edges(&paths)?;
    let graph = reachable_graph(&entry, &edges);
    let d2_boards = boards(&graph);
    let mut node_board = BTreeMap::new();
    for (board_index, board) in d2_boards.iter().enumerate() {
        for node in &board.nodes {
            node_board.insert(node.clone(), board_index);
        }
    }

    std::fs::create_dir_all(&args.out).map_err(|err| err.to_string())?;
    let mut markdown_boards = Vec::new();
    for (index, board_data) in d2_boards.iter().enumerate() {
        let text = board(board_data, index, &graph, &kinds, &node_board);
        let drawn = text.lines().filter(|line| line.contains(" -> ")).count();
        let shapes = text
            .lines()
            .filter(|line| line.contains("{ shape: rectangle;"))
            .count();
        let path = args.out.join(format!("typegraph.{index}.d2"));
        std::fs::write(&path, &text).map_err(|err| err.to_string())?;
        println!(
            "{} layer={} nodes={} shapes={} edges={}",
            path.display(),
            board_data.topo_layer,
            board_data.nodes.len(),
            shapes,
            drawn
        );
        markdown_boards.push((board_data.topo_layer, text));
    }
    if let Some(path) = args.markdown_into.as_deref() {
        regenerate_markdown(path, &args.entry, &markdown_boards)?;
    }
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("typegraph_d2: {err}"); // @eprintln-ok: example CLI usage line
        std::process::exit(2);
    }
}
