//! Prolog families, call resolution and specifier spelling. Five library-API
//! tests folded into `tests/fixtures/prolog_cases/`; every original assert is
//! a claim step there and the whole projection tables freeze in the snapshot.
//! The corpus parse ledger keeps its own test below: its claims bind the
//! external sprefa checkout (`SPREFA_ROOT` or the sibling `sprefa`), the walk
//! floor and the 19-file clean list are live ratchets by contract, and the
//! clean-file complement is deliberately unpinned — a whole-output snapshot
//! would turn corpus growth into a failure the original refused to have.
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("prolog_cases", |case| {
        crate::fixture_runner::commands(case, prolog_api)
    });
}

/// The path label every original extract call passed; the bytes come from the
/// step's fixture file.
const PATH_LABEL: &str = "tests/fixtures/prolog/0_sample.pl";

fn prolog_api(step: &serde_json::Value) -> serde_json::Value {
    use sprefa_extract::{
        build_def_index, content_id_of, flatten, CallEdgeKind, CallF, FamilyMask, FamilyTag,
        FileSet, IndexBag, ManifestMap, ProjectCx, ProjectDigest, PrologSource, Resolve, Source,
    };
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(step["file"].as_str().unwrap()),
    )
    .unwrap();
    let mut rows: Vec<String> = Vec::new();
    match step["api"].as_str().unwrap() {
        "extract" => {
            let output = PrologSource.extract(PATH_LABEL, &bytes, FamilyMask::ALL);
            let facts = flatten(&output);
            match step["rows"].as_str().unwrap() {
                "definitions" => {
                    for fact in &facts {
                        if let sprefa_extract::FlatFact::Node {
                            family: FamilyTag::Call,
                            name: Some(name),
                            ..
                        } = fact
                        {
                            rows.push(format!("{{\"kind\":\"definition\",\"name\":\"{name}\"}}"));
                        }
                    }
                }
                "sites" => {
                    for fact in &facts {
                        if let sprefa_extract::FlatFact::Site { callee, callee_path, .. } = fact {
                            rows.push(match callee_path {
                                Some(path) => format!(
                                    "{{\"callee\":\"{callee}\",\"callee_path\":\"{path}\",\"kind\":\"site\"}}"
                                ),
                                None => format!(
                                    "{{\"callee\":\"{callee}\",\"callee_path\":null,\"kind\":\"site\"}}"
                                ),
                            });
                        }
                    }
                }
                "presence" => {
                    rows.push(format!(
                        "{{\"kind\":\"cst_nodes\",\"present\":{}}}",
                        output
                            .cst
                            .as_ref()
                            .is_some_and(|bundle| !bundle.nodes.is_empty())
                    ));
                    rows.push(format!(
                        "{{\"kind\":\"df_nodes\",\"present\":{}}}",
                        output
                            .df
                            .as_ref()
                            .is_some_and(|bundle| !bundle.nodes.is_empty())
                    ));
                    rows.push(format!(
                        "{{\"kind\":\"df_edges\",\"present\":{}}}",
                        output
                            .df
                            .as_ref()
                            .is_some_and(|bundle| !bundle.edges.is_empty())
                    ));
                }
                other => panic!("unknown extract rows: {other} ({step})"),
            }
        }
        "resolve" => {
            let output = PrologSource.extract(PATH_LABEL, &bytes, FamilyMask::ALL);
            let blob = content_id_of(&bytes);
            let index = build_def_index(&[(blob, &output)]);
            let indexes = IndexBag::default();
            indexes.def_index.set(index).unwrap();
            let files = FileSet;
            let manifests = ManifestMap;
            let cx = ProjectCx {
                files: &files,
                manifests: &manifests,
                reader: None,
                digest: ProjectDigest::default(),
                indexes,
                witness: false,
            };
            for edge in Resolve::<CallF>::resolve(&PrologSource, &output, &cx).iter() {
                let kind = match edge.kind {
                    CallEdgeKind::NameResolve => "NameResolve",
                    CallEdgeKind::ScipOverride => "ScipOverride",
                    CallEdgeKind::ValueRef => "ValueRef",
                    CallEdgeKind::ImportResolve => "ImportResolve",
                    CallEdgeKind::Implements => "Implements",
                    CallEdgeKind::ScipMacro => "ScipMacro",
                    CallEdgeKind::CheckerResolve => "CheckerResolve",
                };
                rows.push(format!("{{\"kind\":\"{kind}\"}}"));
            }
        }
        "specifiers" => {
            let output = PrologSource.extract(PATH_LABEL, &bytes, FamilyMask::ALL);
            let facts = flatten(&output);
            for fact in &facts {
                if let sprefa_extract::FlatFact::Specifier { kind, module, name, .. } = fact {
                    let module = match module {
                        Some(module) => format!("\"{module}\""),
                        None => "null".to_string(),
                    };
                    rows.push(format!(
                        "{{\"kind\":\"{kind}\",\"module\":{module},\"name\":\"{name}\"}}"
                    ));
                }
            }
        }
        other => panic!("unknown api step: {other} ({step})"),
    }
    serde_json::Value::String(rows.join("\n"))
}

/// Only its own job: catch a mis-rooted or empty walk. It is NOT a pin on how
/// many prolog files v6 has, only that the corpus stays non-trivial.
const CORPUS_FLOOR: usize = 60;

/// The ratchet, and the only durable fact in this ledger: these v6 prolog files
/// parse with ZERO error nodes under tree-sitter-prolog today, so `PrologSource`
/// sees a complete tree for them. A file here that starts erroring is a grammar
/// or dependency regression and this list is what catches it. Adding new prolog
/// files never touches this list; renaming or deleting one does, loudly, by the
/// existence check below.
///
/// The complement is deliberately NOT pinned. 52 of the 71 non-lab corpus files
/// carry error nodes (tree-sitter-prolog does not cover the SWI/DCG surface
/// v6/prolog is written in), and `conformance/fixtures/` grows on nearly every
/// arc, so pinning the failing set would go red on additions that say nothing
/// about the grammar. Recovery over those files is asserted directly instead:
/// every file must still yield a tree.
const PARSES_CLEAN: &[&str] = &[
    "0_body_walk.pl",
    "0_unsupported_messages.pl",
    "1_expansion.pl",
    "ARCH.pl",
    "compile/1_emit_registry_docs.pl",
    "compile/2_emit_cli_inventory.pl",
    "6_profile.pl",
    "compile/oracle_dump.pl",
    "compile/registry.pl",
    "compile/scripts/bop_check.pl",
    "compile/scripts/dl6_oracle.pl",
    "compile/scripts/golden_coverage.pl",
    "compile/scripts/golden_oracle.pl",
    "compile/scripts/text_door_receipt.pl",
    "conformance/rulings.pl",
    "src/grader.pl",
    "src/kernel.pl",
    "tools/arch_map.pl",
    "tools/prolog_lint.pl",
];

/// The ledger this test keeps has three parts, each stated as its own assertion
/// so a failure names which one broke:
///  1. RECOVERY: tree-sitter-prolog returns a tree for every corpus file. This
///     is the claim the test name makes and it holds over erroring files too.
///  2. WALK SANITY: the corpus root resolved and was non-trivially populated.
///  3. NO GRAMMAR REGRESSION: every file in `PARSES_CLEAN` still parses clean.
///
/// `labs/` is excluded from the walk. Lab files are transient by the standing
/// lab protocol (labs die on landing, git history is their archive), so their
/// parse status is not a durable fact.
///
/// SABOTAGE RECEIPTS (both run, both red, then reverted):
///  - listing `compile/lower.pl` (a known-erroring file) in `PARSES_CLEAN`
///    -> "now carry error nodes: [\"compile/lower.pl\"]".
///  - listing `compile/renamed_away.pl` (no such file)
///    -> "PARSES_CLEAN names files the corpus no longer has".
#[test]
fn prolog_parser_error_recovery_ledger_for_the_v6_corpus() {
    let Some(corpus) = v6_prolog_corpus() else {
        eprintln!("skipped: no v6 prolog corpus; set SPREFA_ROOT to a sprefa checkout");
        return;
    };
    let corpus = corpus.as_path();
    let mut files = Vec::new();
    collect_prolog_files(corpus, &mut files);
    files.sort();

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter::Language::new(tree_sitter_prolog::LANGUAGE))
        .unwrap();

    let mut clean: Vec<String> = Vec::new();
    let mut erroring = 0usize;
    for path in &files {
        let relative = path.strip_prefix(corpus).unwrap().display().to_string();
        let source = std::fs::read(path).unwrap();
        // 1. RECOVERY: a tree comes back for every file, erroring or not.
        let tree = parser.parse(&source, None).unwrap_or_else(|| {
            panic!("tree-sitter-prolog returned no tree for {relative}: recovery failed")
        });
        if tree.root_node().has_error() {
            erroring += 1;
        } else {
            clean.push(relative);
        }
    }

    // 2. WALK SANITY.
    assert!(
        files.len() >= CORPUS_FLOOR,
        "walked only {} prolog files under {}: the corpus root looks wrong, \
         not the corpus (floor {CORPUS_FLOOR} guards the walk, never the size)",
        files.len(),
        corpus.display(),
    );
    assert_eq!(
        files.len(),
        clean.len() + erroring,
        "every walked file is classified exactly once"
    );

    // 3. NO GRAMMAR REGRESSION, both directions of maintenance made loud.
    let walked: Vec<&str> = files
        .iter()
        .map(|path| path.strip_prefix(corpus).unwrap().to_str().unwrap())
        .collect();
    let missing: Vec<&str> = PARSES_CLEAN
        .iter()
        .copied()
        .filter(|listed| !walked.contains(listed))
        .collect();
    assert!(
        missing.is_empty(),
        "PARSES_CLEAN names files the corpus no longer has: {missing:?}. \
         They were renamed or deleted; update the list."
    );
    let regressed: Vec<&str> = PARSES_CLEAN
        .iter()
        .copied()
        .filter(|listed| !clean.iter().any(|got| got == listed))
        .collect();
    assert!(
        regressed.is_empty(),
        "these files parsed clean when this ledger was written and now carry \
         error nodes: {regressed:?}. tree-sitter-prolog regressed, or the file \
         gained syntax the grammar does not cover."
    );
}

/// The corpus walk. `labs/` is skipped: see the ledger test's header.
fn collect_prolog_files(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "labs") {
                continue;
            }
            collect_prolog_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "pl") {
            files.push(path);
        }
    }
}

/// The corpus lives in the sprefa repository: `SPREFA_ROOT`, else `sprefa`
/// beside the directory that holds this repository's git common dir.
fn v6_prolog_corpus() -> Option<std::path::PathBuf> {
    let root = std::env::var_os("SPREFA_ROOT")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            let output = std::process::Command::new("git")
                .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .output()
                .ok()?;
            let common = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let projects = std::path::Path::new(&common).parent()?.parent()?;
            Some(projects.join("sprefa"))
        })?;
    let corpus = root.join("v6/prolog");
    corpus.is_dir().then_some(corpus)
}
