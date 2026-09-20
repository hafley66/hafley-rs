//! A file two `#[path]` decls name is two modules, and `extract rename` plans
//! the UNION of both routes instead of stopping with exit 6.
//!
//! @comment-ok: plan header, the signatures and lifetimes the change follows.
//! PLAN, `lang/rust_rename.rs`:
//!     homes: BTreeMap<String, Vec<ModuleId>>       appearance-ordered, never empty
//!     fn homes_of(&self, rel: &str) -> &[ModuleId]  a file under no root is the
//!                                                  1-element ORPHAN slice
//!     path_module_table -> BTreeMap<String, Vec<ModuleId>>   one entry per route
//!     harvest(.., home: &ModuleId, anchor_modules: &[ModuleId], ..)
//!     nameable(anchors: &[ModuleId]) -> BTreeSet<ModuleId>
//! Every `Vec<ModuleId>` is built once in `Corpus::open` and borrowed from
//! `&Corpus` for the whole run; nothing outlives `symbol_refs`.
//!
//! The anchor: the module list is unioned FIRST. `nameable` seeds its set with
//! every anchor module and grows one fixpoint, so it equals the union of one
//! fixpoint per module (the step is monotone). `reexports` seeds a FILE scope
//! `(anchor file, chain)`, which is one value for any N, so it stays one call.
//! `variant_leaf` and `owner_reach` compare a resolved module to the anchor
//! module and become `anchor_modules.contains(&module)`.
//!
//! `resolve` keeps its `&ModuleId` signature. The three readers (`nameable`,
//! `reexports`, `harvest`) loop `homes_of(rel)` and call it once per home;
//! `harvest` takes the home as a parameter and `symbol_refs` loops it.
//!
//! Dedupe: `refs` is deduped by `settle` on `(file, span.start)` after the sort
//! by `(file, span.start)`, and `seats` by `(file, span)` in the existing pass.
//! Both sort first, so order is the plan order and never the route order.
//!
//! A file with one module runs each loop once and pushes the same rows in the
//! same order, so its output is byte-identical to the single-home code.
//!
//! @comment-ok: fail-first receipt, repo law keeps these in TEST headers.
//! FAIL-FIRST, against the exit-6 binary: the two-route cases fail with
//!     `expected a plan: exit Some(6)` where a plan is expected.

use std::path::PathBuf;
use std::process::Command;

/// Two crate roots, each a route to `src/bin/home.rs`: the bin root reaches it
/// as `home`, the test root through `../src/bin/home.rs`. Each root imports the
/// symbol through its OWN `crate::home`, so only the union sees both imports.
const TWO_ROUTE: &[(&str, &str)] = &[
    ("src/lib.rs", "pub fn root() {}\n"),
    (
        "src/bin/extract.rs",
        "#[path = \"home.rs\"] mod home;\nuse crate::home::Thing;\nfn from_bin(_: Thing) {}\n",
    ),
    (
        "tests/probe.rs",
        "#[path = \"../src/bin/home.rs\"] mod home;\nuse crate::home::Thing;\nfn from_test(_: Thing) {}\n",
    ),
    (
        "src/bin/home.rs",
        "pub struct Thing;\npub fn make() -> Thing {\n    Thing\n}\n",
    ),
];

/// The same two routes with no importer: every occurrence sits in the reached
/// file, so both routes reach the SAME three spans.
const SAME_OCCURRENCE: &[(&str, &str)] = &[
    ("src/lib.rs", "pub fn root() {}\n"),
    ("src/bin/extract.rs", "#[path = \"home.rs\"] mod home;\n"),
    (
        "tests/probe.rs",
        "#[path = \"../src/bin/home.rs\"] mod home;\n",
    ),
    (
        "src/bin/home.rs",
        "pub struct Thing;\npub fn make() -> Thing {\n    Thing\n}\n",
    ),
];

/// One route: the ordinary single-`#[path]` shape the plan already handled.
const ONE_ROUTE: &[(&str, &str)] = &[
    (
        "src/lib.rs",
        "#[path = \"elsewhere/impl.rs\"]\nmod util;\nuse crate::util::Thing;\n\npub fn build() -> Thing {\n    Thing\n}\n",
    ),
    ("src/elsewhere/impl.rs", "pub struct Thing;\n"),
];

const TS_DYNAMIC: &[(&str, &str)] = &[(
    "src/app.ts",
    "class Widget {}\n\nconst viaComputed = { mark: 1 }[\"Widget\"];\n",
)];

struct Fixture {
    root: PathBuf,
    state: PathBuf,
}

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn fixture(label: &str, files: &[(&str, &str)]) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "extract_rename_path_union_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = base.join("repo");
    let state = base.join("state");
    std::fs::create_dir_all(&state).expect("create state dir");
    for (rel, text) in files {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().expect("rel has a parent")).expect("create dir");
        std::fs::write(path, text).expect("write fixture file");
    }
    Fixture {
        root: root.canonicalize().expect("canonicalize fixture root"),
        state,
    }
}

/// `--commit` rewrites the fixture tree, so the assertions read the real bytes.
fn rename_run(fixture: &Fixture, target: &str, new: &str) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .arg("rename")
        .arg(target)
        .arg(new)
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .arg("--commit")
        .output()
        .expect("extract binary runs");
    Run {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    }
}

/// The plan's `  <file>  <n> uses` rows, in print order.
fn use_rows(run: &Run) -> Vec<String> {
    run.stdout
        .lines()
        .filter(|line| line.starts_with("  ") && line.ends_with(" uses"))
        .map(str::to_string)
        .collect()
}

fn read(fixture: &Fixture, rel: &str) -> String {
    std::fs::read_to_string(fixture.root.join(rel)).expect("read fixture file")
}

/// The defect: two `#[path]` routes to one file stopped the run at exit 6 and
/// emitted no plan.
#[test]
fn a_symbol_two_path_attrs_reach_gets_a_plan_instead_of_a_stop() {
    let fixture = fixture("plan", TWO_ROUTE);
    let run = rename_run(&fixture, "src/bin/home.rs#Thing", "Renamed");
    assert_eq!(
        run.code,
        Some(0),
        "expected a plan: exit {:?}\n{}",
        run.code,
        run.stderr
    );
    assert!(
        !run.stderr.contains("path attr twice"),
        "no double-reach stop line:\n{}",
        run.stderr
    );
}

/// Each root imports the symbol through its own route, so a plan that reads one
/// route misses the other root's two occurrences.
#[test]
fn the_plan_is_the_union_of_both_routes() {
    let fixture = fixture("union", TWO_ROUTE);
    let run = rename_run(&fixture, "src/bin/home.rs#Thing", "Renamed");
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert_eq!(
        use_rows(&run),
        vec![
            "  src/bin/extract.rs  2 uses",
            "  src/bin/home.rs  3 uses",
            "  tests/probe.rs  2 uses",
        ]
    );
    assert_eq!(
        read(&fixture, "src/bin/extract.rs"),
        "#[path = \"home.rs\"] mod home;\nuse crate::home::Renamed;\nfn from_bin(_: Renamed) {}\n"
    );
    assert_eq!(
        read(&fixture, "tests/probe.rs"),
        "#[path = \"../src/bin/home.rs\"] mod home;\nuse crate::home::Renamed;\nfn from_test(_: Renamed) {}\n"
    );
}

/// Both routes read the same three spans in the reached file; `(path,
/// byte_span)` collapses them, so the file is planned and respelled once.
#[test]
fn an_occurrence_both_routes_reach_is_planned_once() {
    let fixture = fixture("dedupe", SAME_OCCURRENCE);
    let run = rename_run(&fixture, "src/bin/home.rs#Thing", "Renamed");
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert_eq!(use_rows(&run), vec!["  src/bin/home.rs  3 uses"]);
    assert_eq!(
        read(&fixture, "src/bin/home.rs"),
        "pub struct Renamed;\npub fn make() -> Renamed {\n    Renamed\n}\n"
    );
}

/// The common path must not move: a one-route file plans the rows the single
/// home code planned, recorded from the binary before the change.
#[test]
fn a_file_with_one_route_plans_exactly_as_before() {
    let fixture = fixture("single", ONE_ROUTE);
    let run = rename_run(&fixture, "src/elsewhere/impl.rs#Thing", "Renamed");
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert_eq!(
        use_rows(&run),
        vec!["  src/elsewhere/impl.rs  1 uses", "  src/lib.rs  3 uses"]
    );
    assert_eq!(
        read(&fixture, "src/lib.rs"),
        "#[path = \"elsewhere/impl.rs\"]\nmod util;\nuse crate::util::Renamed;\n\npub fn build() -> Renamed {\n    Renamed\n}\n"
    );
    assert_eq!(read(&fixture, "src/elsewhere/impl.rs"), "pub struct Renamed;\n");
}

/// Only the double reach is planned now: another `Dynamic` cause still exits 6
/// with its seat and leaves the tree alone.
#[test]
fn a_dynamic_stop_from_another_cause_still_refuses() {
    let fixture = fixture("refuse", TS_DYNAMIC);
    let run = rename_run(&fixture, "src/app.ts#Widget", "Gadget");
    assert_eq!(run.code, Some(6), "Dynamic exits 6:\n{}", run.stderr);
    assert!(
        run.stderr
            .contains("src/app.ts:3: computed member reaches the symbol at runtime"),
        "the computed member seat is named:\n{}",
        run.stderr
    );
    assert_eq!(
        read(&fixture, "src/app.ts"),
        "class Widget {}\n\nconst viaComputed = { mark: 1 }[\"Widget\"];\n"
    );
}
