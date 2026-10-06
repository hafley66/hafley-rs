use crate::v5_normalize::{facet_of, line_of, v6_ported};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use sprefa_extract::{
    build_def_index, byte_range_cached, containing_def_site, content_id_of, covering_def,
    definition_of, dispatch, flatten, join_documents, site_occurrence, CallEdgeKind, CallF,
    ContentId, FamilyMask, FamilyTag, FileSet, FlatFact, GoSource, IndexBag, KotlinSource,
    ManifestMap, NodeRef, ProjectCx, ProjectDigest, ProjectEdge, PythonSource, Resolve, RustSource,
    RyiOutput, ScipGo, ScipJava, ScipRust, ScipSource, ScipTypescript, Span, TsSource, TypeF,
    ZERO_CONTENT_ID,
};

use serde_json::{json, Value};

const PORTED: &[&str] = &[
    "type_node",
    "type_sig",
    "call_def",
    "call_site",
    "df_node",
    "df_edge",
    // df_field/df_lit are graded by content in 18_df_aux_fields_lits.rs;
    // listing them here pins v6 push order to v5's node index, a coupling
    // the user declined (2026-08-16).
    "const_value",
];

fn owner_name(out: &RyiOutput, span: Span) -> String {
    out.types
        .as_ref()
        .and_then(|types| types.nodes.iter().find(|node| node.span == span))
        .and_then(|node| node.name)
        .map(|id| out.strings.lookup(id).to_string())
        .unwrap_or_else(|| format!("<no entity at {}..{}>", span.start, span.end()))
}

fn origin_by_edge(edges: &[ProjectEdge<CallF>]) -> HashMap<OriginKey, &'static str> {
    edges
        .iter()
        .filter(|edge| edge.kind != CallEdgeKind::ValueRef)
        .map(|edge| {
            (
                (
                    edge.src.0,
                    edge.dst_span.start,
                    edge.dst_span.end(),
                    edge.kind.as_str(),
                    edge.dst_blob.clone(),
                ),
                edge.origin.as_str(),
            )
        })
        .collect()
}

fn origin_of(
    edge_origin: &HashMap<OriginKey, &'static str>,
    caller: NodeRef,
    dst: &(ContentId, Span),
    kind: CallEdgeKind,
) -> Option<&'static str> {
    let key = (
        caller.0,
        dst.1.start,
        dst.1.end(),
        kind.as_str(),
        dst.0.clone(),
    );
    edge_origin.get(&key).copied()
}

fn wrong_target_followup_path(
    issues_root: &std::path::Path,
    lang: &str,
    origin: &str,
) -> std::path::PathBuf {
    issues_root
        .join(format!("real-repo-ratchet-wrong-target-{lang}-{origin}"))
        .join("item.md")
}

fn is_wrong_target_followup(path: &std::path::Path, lang: &str, origin: &str) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Some((front_matter, body)) = text
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
    else {
        return false;
    };
    let status = front_matter
        .lines()
        .find_map(|line| line.strip_prefix("status:"))
        .map(str::trim);
    let title = format!("# Ratchet wrong-target follow-up: {lang}/{origin}");
    matches!(status, Some("open" | "in_progress" | "fixed"))
        && body.lines().any(|line| line == title)
        && body.contains("## Reproduction receipt")
}

fn wrong_target_followups_required(
    lang: &str,
    by_origin: &BTreeMap<String, (usize, usize, usize)>,
    issues_root: &std::path::Path,
) -> Vec<String> {
    by_origin
        .iter()
        .filter(|(_, (_, wrong_target, _))| *wrong_target > 0)
        .filter(|(origin, _)| {
            !is_wrong_target_followup(
                &wrong_target_followup_path(issues_root, lang, origin),
                lang,
                origin,
            )
        })
        .map(|(origin, (_, count, _))| format!("{lang}/{origin} ({count})"))
        .collect()
}

fn require_wrong_target_followups(
    lang: &str,
    by_origin: &BTreeMap<String, (usize, usize, usize)>,
    issues_root: &std::path::Path,
) {
    let missing = wrong_target_followups_required(lang, by_origin, issues_root);
    assert!(
        missing.is_empty(),
        "RATCHET_BUMP blocked: nonzero wrong-target classes need follow-up cards first: {}; create issues/real-repo-ratchet-wrong-target-<lang>-<origin>/item.md",
        missing.join(", ")
    );
}

fn pin_ratchet_tsv(lang: &str, by_origin: &BTreeMap<String, (usize, usize, usize)>) {
    static TSV_LOCK: Mutex<()> = Mutex::new(());
    let _guard = TSV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/RATCHET.tsv");
    let parse = |text: &str| {
        text.lines()
            .skip(1)
            .filter(|line| !line.is_empty())
            .map(|line| {
                let f: Vec<&str> = line.split('\t').collect();
                assert_eq!(f.len(), 5, "RATCHET.tsv row needs 5 columns: {line}");
                let cell = |i: usize| f[i].parse::<usize>().expect("RATCHET.tsv cell");
                (
                    f[0].to_string(),
                    f[1].to_string(),
                    cell(2),
                    cell(3),
                    cell(4),
                )
            })
            .collect::<Vec<_>>()
    };
    if matches!(std::env::var("RATCHET_BUMP").as_deref(), Ok("1")) {
        let issues_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("the crate manifest is under the repository root")
            .join("issues");
        require_wrong_target_followups(lang, by_origin, &issues_root);
        let mut rows = parse(&std::fs::read_to_string(&path).unwrap_or_default());
        for (origin, (t, w, u)) in by_origin {
            match rows.iter().position(|r| r.0 == lang && r.1 == *origin) {
                Some(i) => {
                    rows[i].2 = rows[i].2.max(*t);
                    rows[i].3 = rows[i].3.min(*w);
                    rows[i].4 = rows[i].4.min(*u);
                }
                None => rows.push((lang.to_string(), origin.clone(), *t, *w, *u)),
            }
        }
        rows.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        let mut out = String::from("lang\torigin\ttrue\twrong_target\tunresolved\n");
        for (lang, origin, t, w, u) in &rows {
            out.push_str(&format!("{lang}\t{origin}\t{t}\t{w}\t{u}\n"));
        }
        std::fs::write(&path, out).expect("write RATCHET.tsv");
    } else {
        let rows = parse(&std::fs::read_to_string(&path).unwrap_or_default());
        for origin in by_origin.keys() {
            assert!(
                rows.iter().any(|r| r.0 == lang && r.1 == *origin),
                "unpinned histogram row ({lang}, {origin}): run once with RATCHET_BUMP=1"
            );
        }
        for (_, origin, floor, w_ceiling, u_ceiling) in rows.iter().filter(|r| r.0 == lang) {
            let (t, w, u) = by_origin.get(origin).copied().unwrap_or_default();
            let floor_count = if lang == "ts" && origin == "corpus_unique" {
                t + by_origin
                    .get("same_file")
                    .map_or(0, |(same_file, _, _)| *same_file)
            } else {
                t
            };
            assert!(
                floor_count >= *floor,
                "{lang}/{origin}: true {floor_count} below the pinned floor {floor}"
            );
            assert!(
                w <= *w_ceiling,
                "{lang}/{origin}: wrong_target {w} above the pinned ceiling {w_ceiling}"
            );
            assert!(
                u <= *u_ceiling,
                "{lang}/{origin}: unresolved {u} above the pinned ceiling {u_ceiling}"
            );
        }
    }
}

fn short(blob: &ContentId) -> String {
    blob.to_string().chars().take(16).collect()
}

type OriginKey = (u32, u32, u32, &'static str, ContentId);

/// The twin re-derives outcomes without origins; join each row back to the
/// arm edge to meter by resolution origin.
#[derive(Default, serde::Serialize)]
struct RatchetCounts {
    name_resolve: usize,
    scip_override: usize,
    external_no_edge: usize,
    missing_occurrence: usize,
    disagreements: usize,
    misses: usize,
    overbound: usize,
    by_origin: BTreeMap<String, (usize, usize, usize)>,
    join_hits: usize,
}

impl RatchetCounts {
    fn resolved(&mut self, origin: &str) {
        self.by_origin.entry(origin.to_string()).or_default().0 += 1;
    }
    fn wrong_target(&mut self, origin: &str) {
        self.by_origin.entry(origin.to_string()).or_default().1 += 1;
    }
    fn unresolved(&mut self) {
        self.by_origin.entry("none".to_string()).or_default().2 += 1;
    }
}

fn ratchet(case: &Value) -> Value {
    let lang = case["language"].as_str().unwrap();
    let source: &dyn Resolve<CallF> = match lang {
        "ts" => &TsSource,
        "go" => &GoSource,
        "rust" => &RustSource,
        "kotlin" => &KotlinSource,
        _ => panic!("unknown language"),
    };
    let globals = sprefa_extract::lang::ts_lib::globals(None);
    let fixture_root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(case["root"].as_str().unwrap());
    let mut rels: Vec<String> = Vec::new();
    let mut stack = vec![fixture_root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                if !case["skip_directories"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|name| path.file_name().unwrap().to_str() == name.as_str())
                {
                    stack.push(path);
                }
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) == case["extension"].as_str() {
                rels.push(
                    path.strip_prefix(&fixture_root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    rels.sort();
    let reader = |p: &str| std::fs::read(fixture_root.join(p)).ok();
    let indexer: &dyn ScipSource = match case["indexer"].as_str().unwrap() {
        "typescript" => &ScipTypescript,
        "go" => &ScipGo,
        "rust" => &ScipRust,
        "java" => &ScipJava,
        _ => panic!("unknown indexer"),
    };
    let index_path = if case["committed_index"].as_bool().unwrap() {
        let path = fixture_root.join("index.scip");
        assert!(path.is_file(), "committed SCIP fixture is missing");
        path
    } else {
        indexer.build(&fixture_root).expect("real indexer build")
    };
    let scip_index = indexer.load(&index_path).expect("scip load");
    let joined = join_documents(&scip_index, &reader);
    assert!(
        joined.iter().all(Option::is_some),
        "every scip document is reader-readable: the corpus and the index cover the same universe"
    );
    let corpus: Vec<(String, ContentId, Arc<RyiOutput>)> = rels
        .iter()
        .map(|rel| {
            let bytes = reader(rel).unwrap();
            (
                rel.clone(),
                content_id_of(&bytes),
                dispatch(rel, &bytes, FamilyMask::ALL).expect("a Source matches the fixture"),
            )
        })
        .collect();
    let pairs: Vec<(ContentId, &RyiOutput)> = corpus
        .iter()
        .map(|(_, hash, out)| (hash.clone(), out.as_ref()))
        .collect();
    let file_set = FileSet;
    let manifest_map = ManifestMap;
    let cx = ProjectCx {
        files: &file_set,
        manifests: &manifest_map,
        reader: Some(&reader),
        digest: ProjectDigest::default(),
        indexes: IndexBag::default(),
        witness: false,
    };
    cx.indexes
        .def_index
        .set(build_def_index(&pairs))
        .expect("fresh OnceLock");
    cx.indexes
        .scip_index
        .set(scip_index)
        .expect("fresh OnceLock");
    let scip_index = cx.indexes.scip_index.get().unwrap();
    let def_index = cx.indexes.def_index.get().unwrap();

    if lang == "rust" {
        let names = hafley_scm::read::lang::rust_names_index::RustNamesIndex::build_in(
            &fixture_root,
            corpus
                .iter()
                .filter_map(|(rel, _, out)| {
                    out.rust_module.clone().map(|facts| (rel.clone(), facts))
                })
                .collect(),
            &corpus
                .iter()
                .map(|(rel, blob, _)| (rel.clone(), blob.clone()))
                .collect::<Vec<_>>(),
            def_index,
        );
        cx.indexes
            .paths
            .set(hafley_scm::read::types::build_path_index(
                corpus
                    .iter()
                    .map(|(rel, blob, _)| (blob.clone(), rel.as_str())),
            ))
            .unwrap();
        cx.indexes.rust_modules.set(names).ok().unwrap();
    }
    let mut observed = BTreeMap::new();
    let mut total_sites = 0usize;
    let mut counts = RatchetCounts::default();
    let mut lines: Vec<String> = Vec::new();
    for (rel, blob, out) in &corpus {
        let doc_ix = scip_index
            .documents
            .iter()
            .position(|d| &d.relative_path == rel)
            .expect("one scip document per fixture file");
        let doc = &scip_index.documents[doc_ix];
        let content = reader(rel).unwrap();
        let Some(call) = &out.call else { continue };
        let edges = source.resolve(out, &cx);
        let edge_origin = origin_by_edge(&edges);
        let mut actual: Vec<(u32, u32, u32, &'static str, ContentId)> = edges
            .iter()
            .filter(|edge| edge.kind != CallEdgeKind::ValueRef)
            .map(|edge| {
                let from = call.node(edge.src).span;
                (
                    from.start,
                    edge.dst_span.start,
                    edge.dst_span.end(),
                    edge.kind.as_str(),
                    edge.dst_blob.clone(),
                )
            })
            .collect();
        actual.sort_by_key(|t| (t.0, t.1, t.2, t.3));
        let mut expected: Vec<(u32, u32, u32, &'static str, ContentId)> = Vec::new();
        for site in &call.aux.sites {
            total_sites += 1;
            let callee = out.strings.lookup(site.callee);
            let line = line_of(&content, site.span.start);
            let occ = site_occurrence(doc, &content, site.span, callee);
            if occ.is_some() {
                counts.join_hits += 1;
            } else {
                counts.missing_occurrence += 1;
                lines.push(format!("MISSING-OCCURRENCE {rel}:{line} {callee}"));
            }
            let scip_t = occ
                .filter(|sym| {
                    !case["exclude_local_symbols"].as_bool().unwrap()
                        || !scip_index.symbol(*sym).starts_with("local ")
                })
                .and_then(|sym| definition_of(scip_index, doc_ix, sym))
                .and_then(|(def_doc_ix, def_range)| {
                    let def_doc = &scip_index.documents[def_doc_ix];
                    let (def_blob, def_content) = joined[def_doc_ix].as_ref().unwrap();
                    let ident = byte_range_cached(
                        def_doc,
                        def_content,
                        def_range,
                        def_doc.position_encoding,
                    )?;
                    containing_def_site(def_index, def_blob.clone(), ident)
                        .map(|(name, s)| (def_blob.clone(), s.span, name))
                });
            let name_t = match lang {
                "ts" => TsSource::call_lexical_match(out, site, blob, None).or_else(|| {
                    TsSource::call_name_match(out, def_index, callee, Some(blob), globals.as_ref())
                }),
                "go" => GoSource::call_name_match(out, def_index, callee),
                "kotlin" => KotlinSource::call_name_match(out, def_index, callee),
                "rust" => {
                    let method = call
                        .aux
                        .receivers
                        .iter()
                        .any(|receiver| receiver.call_site == site.span);
                    let written = site
                        .callee_path
                        .map(|id| out.strings.lookup(id))
                        .unwrap_or(callee);
                    (!method)
                        .then(|| {
                            let names = cx.indexes.rust_modules.get()?;
                            let bound = names
                                .binding_at(
                                    rel,
                                    &written.split("::").map(str::to_string).collect::<Vec<_>>(),
                                    Some(site.span.start),
                                    FamilyTag::Call,
                                )
                                .ok()?;
                            if bound.target_name.is_none()
                                || names.is_collapsed(&bound.target_blob, bound.target_span)
                                || names.is_alias(&bound.target_blob, bound.target_span)
                            {
                                return None;
                            }
                            Some((bound.target_blob, bound.target_span))
                        })
                        .flatten()
                }
                _ => unreachable!(),
            };
            let twin = covering_def(call, site.span).and_then(|caller| {
                let (dst, kind) = match (name_t.clone(), scip_t.clone()) {
                    (Some(n), Some(s)) if n.0 == s.0 && callee == s.2 => {
                        (n, CallEdgeKind::NameResolve)
                    }
                    (_, Some(s)) => ((s.0, s.1), CallEdgeKind::ScipOverride),
                    (Some(n), None) => (n, CallEdgeKind::NameResolve),
                    (None, None) => return None,
                };
                Some((caller, dst, kind))
            });
            let mut origin: Option<&'static str> = None;
            if let Some((caller, dst, kind)) = &twin {
                let from = call.node(*caller).span;
                origin = origin_of(&edge_origin, *caller, dst, *kind);
                expected.push((
                    from.start,
                    dst.1.start,
                    dst.1.end(),
                    kind.as_str(),
                    dst.0.clone(),
                ));
            }
            match (twin, scip_t) {
                (Some((_, dst, CallEdgeKind::NameResolve)), Some(s)) => {
                    if !(dst.0 == s.0 && callee == s.2) {
                        counts.disagreements += 1;
                        if let Some(o) = origin {
                            counts.wrong_target(o);
                        }
                        lines.push(format!(
                            "DISAGREE {rel}:{line} {callee}: v6 NameResolve -> ({:?}, {callee}), scip -> ({:?}, {})",
                            short(&dst.0), short(&s.0), s.2
                        ));
                    } else {
                        counts.name_resolve += 1;
                        if let Some(o) = origin {
                            counts.resolved(o);
                        }
                    }
                }
                (Some((_, dst, CallEdgeKind::NameResolve)), None) => {
                    counts.overbound += 1;
                    lines.push(format!(
                        "OVERBOUND {rel}:{line} {callee}: v6 NameResolve -> ({:?}) but scip has no corpus target",
                        short(&dst.0)
                    ));
                }
                (Some((_, dst, CallEdgeKind::ScipOverride)), Some(s)) => {
                    assert_eq!(
                        (dst.0.clone(), dst.1),
                        (s.0.clone(), s.1),
                        "override edge carries scip's target at {rel}:{line} {callee}"
                    );
                    assert!(
                        !(name_t == Some((s.0.clone(), s.1)) && callee == s.2),
                        "override with a matching name-match is no override at {rel}:{line} {callee}"
                    );
                    counts.scip_override += 1;
                    if let Some(o) = origin {
                        counts.resolved(o);
                    }
                    lines.push(format!(
                        "OVERRIDE {rel}:{line} {callee}: name-match {} displaced; scip -> ({:?}, {})",
                        match name_t {
                            Some((b, _)) => format!("({:?}, {callee})", short(&b)),
                            None => "<none: ambiguous/absent>".to_string(),
                        },
                        short(&s.0),
                        s.2
                    ));
                }
                (Some((_, _, CallEdgeKind::ScipOverride)), None) => {
                    panic!("override without a scip corpus target at {rel}:{line} {callee}");
                }
                (Some((_, _, CallEdgeKind::ValueRef)), _) => {}
                (Some((_, _, CallEdgeKind::ImportResolve)), _) => {}
                (Some((_, _, CallEdgeKind::Implements)), _) => {}
                (Some((_, _, CallEdgeKind::ScipMacro)), _) => {}
                (Some((_, _, CallEdgeKind::CheckerResolve)), _) => {}
                (None, Some(s)) => {
                    counts.misses += 1;
                    counts.unresolved();
                    lines.push(format!(
                        "MISS {rel}:{line} {callee}: scip resolves to corpus ({:?}, {}) but v6 emitted no edge",
                        short(&s.0), s.2
                    ));
                }
                (None, None) => {
                    if occ.is_some() {
                        counts.external_no_edge += 1;
                    }
                }
            }
        }
        expected.sort_by_key(|t| (t.0, t.1, t.2, t.3));
        assert_eq!(
            actual, expected,
            "[{rel}] arm edges != twin expected outcomes"
        );
        observed.insert(rel.clone(), json!({"actual":actual,"expected":expected}));
        eprintln!(
            "[{rel}] scip ratchet: sites {} | name_resolve {} scip_override {} external-no-edge {}",
            call.aux.sites.len(),
            counts.name_resolve,
            counts.scip_override,
            counts.external_no_edge
        );
    }
    eprintln!(
        "[{lang}-total] scip ratchet ({}) over {} sites: name_resolve {} scip_override {} external-no-edge {} | missing-occurrence {} disagreements {} misses {} overbound {}",
        scip_index.tool(), total_sites, counts.name_resolve, counts.scip_override,
        counts.external_no_edge, counts.missing_occurrence, counts.disagreements,
        counts.misses, counts.overbound
    );
    eprintln!("lang\torigin\ttrue\twrong_target\tunresolved");
    for (origin, (t, w, u)) in &counts.by_origin {
        eprintln!("{lang}\t{origin}\t{t}\t{w}\t{u}");
    }
    assert!(
        counts.join_hits > 0,
        "{lang}: zero join coverage: not one site joined to a scip occurrence"
    );
    pin_ratchet_tsv(lang, &counts.by_origin);
    for line in &lines {
        eprintln!("  {line}");
    }
    assert_eq!(
        counts.missing_occurrence,
        0,
        "occurrence parity: every v6 site has a scip occurrence\n{}",
        lines.join("\n")
    );
    assert_eq!(
        counts.disagreements,
        0,
        "every NameResolve agrees with scip's corpus target\n{}",
        lines.join("\n")
    );
    assert_eq!(
        counts.misses,
        0,
        "no silent misses: every scip-corpus-resolved site has a v6 edge\n{}",
        lines.join("\n")
    );
    assert_eq!(
        counts.overbound,
        0,
        "no overbinding: every NameResolve is scip-corpus-resolved\n{}",
        lines.join("\n")
    );
    json!({"files":observed,"total_sites":total_sites,"counts":counts,"diagnostics":lines})
}

fn corpus(config: &Value) -> Value {
    let cases = config["cases"].as_array().unwrap();
    let corpus: Vec<_> = cases
        .iter()
        .map(|case| {
            let bytes = std::fs::read(test_root().join(case["fixture"].as_str().unwrap())).unwrap();
            let out = dispatch(case["path"].as_str().unwrap(), &bytes, FamilyMask::ALL).unwrap();
            (content_id_of(&bytes), out, case)
        })
        .collect();
    let pairs: Vec<_> = corpus
        .iter()
        .map(|(blob, out, _)| (blob.clone(), out.as_ref()))
        .collect();
    let cx = ProjectCx {
        files: &FileSet,
        manifests: &ManifestMap,
        reader: None,
        digest: ProjectDigest::default(),
        indexes: IndexBag::default(),
        witness: false,
    };
    cx.indexes.def_index.set(build_def_index(&pairs)).unwrap();
    let mut observed = BTreeMap::new();
    for (_, out, case) in &corpus {
        let path = case["path"].as_str().unwrap();
        let lang = case["fixture_dir"].as_str().unwrap();
        let baseline =
            std::fs::read_to_string(test_root().join(case["baseline"].as_str().unwrap())).unwrap();
        let bytes = std::fs::read(test_root().join(case["fixture"].as_str().unwrap())).unwrap();
        let expected: BTreeSet<_> = baseline
            .lines()
            .filter(|line| PORTED.contains(&facet_of(line)))
            .map(str::to_string)
            .collect();
        let anchors = crate::df_increment_support::oracle_starts(&baseline);
        let actual: BTreeSet<_> = v6_ported(path, &bytes)
            .into_iter()
            .filter(|line| PORTED.contains(&facet_of(line)))
            .filter(|line| {
                if lang != "ts" {
                    return true;
                }
                let f: Vec<_> = line.split('\t').collect();
                match f[0] {
                    "df_node" => anchors.contains(&f[3].parse().unwrap()),
                    "df_edge" => {
                        anchors.contains(&f[1].parse().unwrap())
                            && anchors.contains(&f[2].parse().unwrap())
                    }
                    _ => true,
                }
            })
            .collect();
        assert_eq!(actual, expected, "ported oracle: {path}");
        let mut row = json!({"ported":actual});
        if lang == "rust" && case["name"] == "rust_docs" {
            let doc_expected: BTreeSet<_> = baseline
                .lines()
                .filter(|line| facet_of(line) == "doc")
                .map(str::to_string)
                .collect();
            assert!(!doc_expected.is_empty(), "oracle must carry doc rows");
            let docs: BTreeSet<_> = v6_ported(path, &bytes)
                .into_iter()
                .filter(|line| facet_of(line) == "doc")
                .collect();
            assert_eq!(docs, doc_expected, "doc oracle");
            row["docs"] = json!(docs);
        }
        if matches!(lang, "ts" | "go" | "python" | "rust") {
            let (edges, candidates) = match lang {
                "ts" => (
                    Resolve::<TypeF>::resolve(&TsSource, out, &cx),
                    TsSource::type_edge_candidates(out),
                ),
                "go" => (
                    Resolve::<TypeF>::resolve(&GoSource, out, &cx),
                    GoSource::type_edge_candidates(out),
                ),
                "python" => (
                    Resolve::<TypeF>::resolve(&PythonSource, out, &cx),
                    PythonSource::type_edge_candidates(out),
                ),
                "rust" => (
                    Resolve::<TypeF>::resolve(&RustSource, out, &cx),
                    RustSource::type_edge_candidates(out),
                ),
                _ => unreachable!(),
            };
            let mut actual: BTreeSet<_> = edges
                .iter()
                .zip(candidates.iter())
                .map(|(_, candidate)| {
                    format!(
                        "type_edge\t{}\t{}\t{}",
                        owner_name(out, candidate.owner),
                        out.strings.lookup(candidate.to),
                        candidate.kind.as_str()
                    )
                })
                .filter(|line| {
                    let f: Vec<_> = line.split('\t').collect();
                    if f[1] == f[2] && f[3] == "uses" {
                        return false;
                    }
                    lang != "rust"
                        || (!config["rust_type_exclusions"]["kinds"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|k| k == f[3])
                            && !(f[3] == "uses"
                                && config["rust_type_exclusions"]["uses"]
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .any(|pair| pair[0] == f[1] && pair[1] == f[2])))
                })
                .collect();
            if edges.len() != candidates.len() {
                actual.insert(format!(
                    "ZIP_MISMATCH edges={} candidates={}",
                    edges.len(),
                    candidates.len()
                ));
            }
            let expected: BTreeSet<_> = baseline
                .lines()
                .filter(|line| facet_of(line) == "type_edge")
                .map(str::to_string)
                .collect();
            assert_eq!(actual, expected, "type oracle: {path}");
            row["type_edges"] = json!(actual);
        }
        report_ledger(lang, case["name"].as_str().unwrap(), &baseline, out, &cx);
        observed.insert(format!("{lang}/{}", case["name"].as_str().unwrap()), row);
    }
    json!(observed)
}

// These migration metrics retain their existing informational status.
fn report_ledger(lang: &str, name: &str, baseline: &str, out: &RyiOutput, cx: &ProjectCx) {
    let mut deferred = BTreeMap::new();
    for line in baseline.lines() {
        let facet = facet_of(line);
        if !PORTED.contains(&facet)
            && !(matches!(lang, "ts" | "go" | "rust" | "python") && facet == "type_edge")
        {
            *deferred.entry(facet).or_insert(0usize) += 1;
        }
    }
    let facts = flatten(out);
    let cst_only = facts
        .iter()
        .filter(|fact| {
            matches!(
                fact,
                FlatFact::Node {
                    family: FamilyTag::Cst,
                    ..
                } | FlatFact::Edge {
                    family: FamilyTag::Cst,
                    ..
                }
            )
        })
        .count();
    let specifier_only = facts
        .iter()
        .filter(|fact| matches!(fact, FlatFact::Specifier { .. }))
        .count();
    let (types, calls) = match lang {
        "ts" => (
            Resolve::<TypeF>::resolve(&TsSource, out, cx),
            Resolve::<CallF>::resolve(&TsSource, out, cx),
        ),
        "go" => (
            Resolve::<TypeF>::resolve(&GoSource, out, cx),
            Resolve::<CallF>::resolve(&GoSource, out, cx),
        ),
        "rust" => (
            Resolve::<TypeF>::resolve(&RustSource, out, cx),
            Resolve::<CallF>::resolve(&RustSource, out, cx),
        ),
        _ => (Vec::new(), Vec::new()),
    };
    let resolved_legs = types
        .iter()
        .filter(|edge| edge.dst_blob != ZERO_CONTENT_ID)
        .count();
    let name_resolve = calls
        .iter()
        .filter(|edge| {
            matches!(
                edge.kind,
                CallEdgeKind::NameResolve | CallEdgeKind::ImportResolve
            )
        })
        .count();
    let scip_override = calls
        .iter()
        .filter(|edge| edge.kind == CallEdgeKind::ScipOverride)
        .count();
    eprintln!("[{name}] migration ledger: v5-only deferred {deferred:?}; v6-only cst facts {cst_only}; v6-only specifier facts {specifier_only}; v6-only resolved type_edge legs {resolved_legs}; v6-only call edges name_resolve {name_resolve} scip_override {scip_override} (no scip loaded)");
}

fn test_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

pub fn evaluate(case: &Value) -> Value {
    match case["operation"].as_str().unwrap() {
        "corpus" => corpus(case),
        "ratchet" => ratchet(case),
        "followups" => {
            let issues = tempfile::tempdir().unwrap();
            let lang = case["language"].as_str().unwrap();
            let histogram = serde_json::from_value(case["histogram"].clone()).unwrap();
            let card = wrong_target_followup_path(issues.path(), lang, "same_file");
            let mut result = Vec::new();
            for status in case["statuses"].as_array().unwrap() {
                if let Some(status) = status.as_str() {
                    std::fs::create_dir_all(card.parent().unwrap()).unwrap();
                    std::fs::write(
                        &card,
                        format!(
                            "---\nstatus: {status}\n---\n{}\n\n{}",
                            case["title"].as_str().unwrap(),
                            case["body"].as_str().unwrap()
                        ),
                    )
                    .unwrap();
                }
                let required = wrong_target_followups_required(lang, &histogram, issues.path());
                if status == "open" {
                    require_wrong_target_followups(lang, &histogram, issues.path());
                }
                result.push(json!({"status":status,"required":required}));
            }
            json!(result)
        }
        "missing_origin" => {
            let histogram = serde_json::from_value(case["histogram"].clone()).unwrap();
            let outcome = std::panic::catch_unwind(|| {
                pin_ratchet_tsv(case["language"].as_str().unwrap(), &histogram)
            });
            let text = match outcome {
                Ok(()) => panic!("a missing origin passed the floor"),
                Err(payload) => payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default(),
            };
            assert!(
                text.contains(case["panic_contains"].as_str().unwrap()),
                "unexpected panic text: {text}"
            );
            json!({"panic":text})
        }
        _ => panic!("unknown operation"),
    }
}
