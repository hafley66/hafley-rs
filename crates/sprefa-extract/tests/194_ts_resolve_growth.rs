//! Span-entry growth of the TS resolve paths at n and 100n: package discovery,
//! workspace resolve, the checker edge path, callee tokens, closure sweeps.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use hafley_observe::{assert_growth_sized, CountRecorder, Growth, SpanCounts};
use sprefa_extract::edit::checker_edges::{CheckerDefs, CheckerEdge};
use sprefa_extract::edit::ts_cst_tokens::CstTokens;
use sprefa_extract::lang::ts_resolve::{module_facts, TsModuleIndex};
use sprefa_extract::{
    content_id_of, dispatch::dispatch_uncached, resolve_project_with_raw, ContentId, DefIndex, FamilyMask,
    FamilyTag, FlatFact, ResolveArms, ResolveRequest, ScipMode, ScipRecords, TsResolver, TsSource,
};
use tracing_subscriber::prelude::*;

static SEQ: AtomicU64 = AtomicU64::new(0);

fn stage(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "sprefa_growth_{label}_{}_{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    root.canonicalize().unwrap()
}

fn counted<T>(run: impl FnOnce() -> T) -> (T, SpanCounts) {
    let (recorder, layer) = CountRecorder::new();
    let out = tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), run);
    (out, recorder.counts())
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// `n` workspace packages `@w/p<i>`, each exporting `src` under the `source` condition.
fn workspace(n: usize) -> PathBuf {
    let root = stage("workspace");
    for i in 0..n {
        let package = root.join(format!("packages/p{i}"));
        write(
            &package.join("package.json"),
            &format!(r#"{{"name":"@w/p{i}","exports":{{".":{{"source":"./src/index.ts","types":"./dist/index.d.ts"}},"./*":{{"source":"./src/*.ts","types":"./dist/*.d.ts"}}}}}}"#),
        );
        write(&package.join("src/index.ts"), &format!("export const value{i} = {i};\n"));
        write(&package.join("src/leaf.ts"), "export const leaf = 1;\n");
    }
    root
}

#[test]
fn package_discovery_and_workspace_resolve_grow_linearly_in_packages() {
    let run = |n: usize| {
        let root = workspace(n);
        let (resolver, discovery) = counted(|| TsResolver::new(&root).unwrap());
        let from = root.join("packages/p0/src/index.ts");
        let (rows, resolve) = counted(|| {
            (0..n)
                .map(|i| resolver.resolve_with_rung(&from, &format!("@w/p{i}")))
                .collect::<Vec<_>>()
        });
        let last = rows.last().cloned().flatten().unwrap();
        assert_eq!(
            (last.0.strip_prefix(&root).unwrap().to_path_buf(), last.1),
            (PathBuf::from(format!("packages/p{}/src/index.ts", n - 1)), "workspace_package")
        );
        std::fs::remove_dir_all(&root).unwrap();
        (discovery, resolve)
    };
    let ((small_discovery, small_resolve), (large_discovery, large_resolve)) = (run(10), run(1000));
    assert_growth_sized(&small_discovery, &large_discovery, "ts.packages.directory", 10, 1000, Growth::Linear);
    assert_growth_sized(&small_resolve, &large_resolve, "ts.resolve.specifier", 10, 1000, Growth::Linear);
}

#[test]
fn one_directory_resolves_a_specifier_once() {
    let root = workspace(1);
    let index = |n: usize| {
        let files: Vec<(String, _)> = (0..n)
            .map(|i| {
                let path = root.join(format!("packages/p0/src/use{i}.ts"));
                let text = format!("import {{ value0 }} from \"@w/p0\";\nexport const use{i} = value0;\n");
                write(&path, &text);
                let path = path.to_string_lossy().into_owned();
                let facts = module_facts(&path, text.as_bytes()).unwrap();
                (path, facts)
            })
            .collect();
        let corpus: Vec<(String, ContentId)> = files
            .iter()
            .map(|(path, _)| (path.clone(), content_id_of(path.as_bytes())))
            .chain(std::iter::once({
                let index = root.join("packages/p0/src/index.ts").to_string_lossy().into_owned();
                let blob = content_id_of(index.as_bytes());
                (index, blob)
            }))
            .collect();
        counted(|| TsModuleIndex::build(files, &corpus, &DefIndex::default())).1
    };
    assert_growth_sized(&index(10), &index(1000), "ts.resolve.specifier", 10, 1000, Growth::Constant);
    std::fs::remove_dir_all(&root).unwrap();
}

/// `n` functions 100 bytes apart; every function holds one site calling the next.
fn checker_run(n: usize) -> SpanCounts {
    let blob = content_id_of(b"growth");
    let mut defs = CheckerDefs::default();
    let mut facts = Vec::new();
    for i in 0..n as u32 {
        defs.push("a.ts", &blob, i * 100, i * 100 + 90, Some(format!("f{i}")), true);
        defs.push("a.ts", &blob, i * 100 + 20, i * 100 + 40, None, true);
        // Half the sites carry a fast row the checker rewrites.
        if i % 2 == 0 {
            facts.push(FlatFact::ResolvedEdge {
                fact: None,
                caller_path: "a.ts".to_string(),
                caller_name: Some(format!("f{i}")),
                caller_site_start: i * 100 + 10,
                caller_site_end: i * 100 + 15,
                callee_path: "b.ts".to_string(),
                callee_name: None,
                callee_start: 0,
                callee_end: 0,
                kind: "name_resolve".to_string(),
                resolution_origin: "corpus_unique".to_string(),
            });
        }
    }
    let (rows, counts) = counted(|| {
        defs.seal();
        let edges: Vec<_> = (0..n as u32)
            .map(|i| CheckerEdge {
                source: "a.ts".to_string(),
                site_start: i * 100 + (if i % 2 == 0 { 10 } else { 25 }),
                site_end: i * 100 + (if i % 2 == 0 { 15 } else { 30 }),
                target: defs.target("a.ts", ((i + 1) % n as u32) * 100 + 1, None).unwrap(),
            })
            .collect();
        defs.write(&mut facts, edges);
        facts
    });
    let checked = rows
        .iter()
        .filter(|fact| matches!(fact, FlatFact::ResolvedEdge { resolution_origin, .. } if resolution_origin == "checker"))
        .count();
    assert_eq!(checked, n);
    // An odd site sits in the unnamed callable nested in f<i>: a closure caller.
    assert!(rows.iter().any(|fact| matches!(fact,
        FlatFact::ResolvedEdge { caller_name: Some(name), caller_site_start: 125, .. } if name != "f1")));
    counts
}

#[test]
fn the_checker_edge_path_grows_linearly_in_sites() {
    let (small, large) = (checker_run(20), checker_run(2000));
    for span in ["checker.defs.link", "checker.defs.step", "checker.edge.site", "checker.edge.row"] {
        assert_growth_sized(&small, &large, span, 20, 2000, Growth::Linear);
    }
}

fn request(paths: &[PathBuf]) -> ResolveRequest<'_> {
    ResolveRequest {
        paths,
        arms: ResolveArms { call: true, types: true, flow: false },
        scip: ScipMode::Off,
        project_root: None,
        scip_records: ScipRecords::all(),
        occurrence_text: false,
        rust_checker: None,
        ts_checker: None,
        go_checker: None,
        witness: false,
    }
}

/// One TSX file with `n` blocks of plain, member, `new` and JSX calls.
fn calls_source(n: usize) -> String {
    (0..n)
        .map(|i| {
            format!(
                "function f{i}() {{ return {i}; }}\nclass C{i} {{ m{i}() {{ return f{i}(); }} }}\n\
                 export function use{i}(o: C{i}) {{ return o.m{i}() + new C{i}().m{i}() + f{i}(); }}\n\
                 const Comp{i} = (p: {{ a: number }}) => null;\nexport const v{i} = <Comp{i} a={{1}} />;\n"
            )
        })
        .collect()
}

fn callee_run(n: usize) -> SpanCounts {
    let root = stage("callee");
    let path = root.join("calls.tsx");
    let text = calls_source(n);
    write(&path, &text);
    let paths = vec![path];
    let mut cst = CstTokens::default();
    let mut sites = Vec::new();
    resolve_project_with_raw(&request(&paths), &mut |raw| {
        cst.push(&raw.fact);
        if let FlatFact::Site { family: FamilyTag::Call, span, callee, .. } = &raw.fact {
            sites.push((span.start, span.end, callee.clone()));
        }
        Ok::<(), std::convert::Infallible>(())
    })
    .unwrap();
    let (tokens, counts) = counted(|| {
        cst.seal();
        sites
            .iter()
            .filter_map(|(start, end, name)| cst.callee(&text, *start, *end, name).map(|token| (name, token)))
            .collect::<Vec<_>>()
    });
    assert_eq!(tokens.len(), sites.len(), "every site has a callee token");
    for (name, (start, end)) in &tokens {
        assert_eq!(&text[*start as usize..*end as usize], name.as_str());
    }
    assert_eq!(cst.attribute_names().count(), n);
    std::fs::remove_dir_all(&root).unwrap();
    counts
}

#[test]
fn callee_tokens_come_from_the_cst_in_linear_steps() {
    assert_growth_sized(&callee_run(5), &callee_run(500), "ts.callee.step", 5, 500, Growth::Linear);
}

fn sweep_run(n: usize) -> SpanCounts {
    let text: String = (0..n)
        .map(|i| format!("export function g{i}() {{ function keep(y: number) {{ return y > 0; }} return [{i}].map((x) => x + 1).filter(keep); }}\n"))
        .collect();
    let (nested, counts) = counted(|| {
        let output = dispatch_uncached("sweep.ts", text.as_bytes(), FamilyMask::ALL).unwrap();
        TsSource::nested_callables(&output)
    });
    assert_eq!(nested.len(), n, "each `keep` is nested in its g<i>");
    counts
}

#[test]
fn closure_owner_and_nested_callable_sweeps_grow_linearly_in_nodes() {
    let (small, large) = (sweep_run(10), sweep_run(1000));
    for span in ["ts.closure_owner.visit", "ts.closure_owner.pop", "ts.nested_callables.visit", "ts.nested_callables.pop"] {
        assert_growth_sized(&small, &large, span, 10, 1000, Growth::Linear);
    }
}
