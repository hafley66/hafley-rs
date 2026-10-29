//! Cost against input size, 3 runs per size (hafley-observe `growth_bench`): a deterministic count
//! (rows written + SQLite VM steps) and the wall-time median, each under an exponent ceiling.
use hafley_observe::growth_bench::{assert_growth_exponent, growth_table, sample_growth};
use hafley_observe::sqlite_work::WorkTrace;

use crate::t_195_scmpp_growth::{scmpp, sqlite};

const RUNS: usize = 3;

fn object(n: usize, value: impl Fn(usize) -> usize) -> String {
    let keys: String = (0..n).map(|key| format!("  k{key}: {},\n", value(key))).collect();
    format!("export const big = {{\n{keys}}};\n")
}

type Compile = fn(&tree_sitter::Language, &str) -> Result<hafley_scm::scmpp::Compiled, hafley_scm::scmpp::ScmppError>;

/// Compile, store and query one source in-process; the count every run reports the same.
fn scmpp_work(program: &str, src: &str) -> u64 {
    scmpp_work_with(hafley_scm::scmpp::compile, program, src)
}

fn scmpp_work_with(compile: Compile, program: &str, src: &str) -> u64 {
    let grammar = sprefa_extract::RyiLang::parse_name("tsx").unwrap().tree_sitter_language();
    let compiled = compile(&grammar, program).unwrap();
    let tree = hafley_scm::cst::parse(&grammar, src.as_bytes()).unwrap();
    let mut db = sqlite::Database::memory().unwrap();
    let mut store = scmpp::open(&mut db).unwrap();
    scmpp::write_file(&mut db, &mut store, &compiled, "bench.tsx", src.as_bytes(), &tree, &|| false, true).unwrap();
    let trace = WorkTrace::start(db.connection()).unwrap();
    scmpp::run_sql(&mut db, &compiled).unwrap();
    let steps = trace.work().vm_steps;
    drop(trace);
    db.rows as u64 + steps
}

/// The 3-sibling split: C(n, 3) result rows, so the output itself is cubic; nothing above it.
#[test]
fn growth_bench_sibling_split_is_output_bound() {
    let program = "(object (pair) @a (pair) @b (pair) @c)";
    let samples = sample_growth(&[20, 40, 80], RUNS, |n| scmpp_work(program, &object(n, |key| key)));
    eprintln!("{}", growth_table("sibling_split", &samples));
    assert_growth_exponent("sibling_split", &samples, 3.2, 3.6);
}

/// A cross-child `#eq?` over distinct values: no result row, so cost is the engine's own.
#[test]
fn growth_bench_cross_eq_split() {
    let program = "((object (pair value: (_) @va) (pair) @b (pair value: (_) @vc)) (#eq? @va @vc))";
    let samples = sample_growth(&[20, 40, 80], RUNS, |n| scmpp_work(program, &object(n, |key| key)));
    eprintln!("{}", growth_table("cross_eq_split", &samples));
    assert_growth_exponent("cross_eq_split", &samples, 2.2, 2.6);
}

/// The rules-of-hooks program over `n` components, one hook call in a branch each: linear.
#[test]
fn growth_bench_hooks_program_is_linear() {
    let program = include_str!("../bench/rules-of-hooks/2_hooks.scm");
    let source = |n: usize| -> String {
        (0..n)
            .map(|i| format!("export function C{i}() {{\n  if (x) {{ useHook{i}(); }}\n  return null;\n}}\n"))
            .collect()
    };
    let samples = sample_growth(&[100, 200, 400], RUNS, |n| scmpp_work(program, &source(n)));
    eprintln!("{}", growth_table("hooks_program", &samples));
    assert_growth_exponent("hooks_program", &samples, 1.2, 1.5);
}

/// `--timeout 1` on an unsplit 3-sibling pattern: the stop lands inside matching, so its latency
/// does not grow with the object; the count is the exit code.
#[test]
fn growth_bench_timeout_latency_is_flat() {
    let dir = tempfile::tempdir().unwrap();
    let pattern = dir.path().join("cross.scm");
    std::fs::write(&pattern, "(object (pair) @a (pair)* @b (pair) @c)\n").unwrap();
    let samples = sample_growth(&[120, 240, 480], RUNS, |n| {
        let file = dir.path().join(format!("o{n}.ts"));
        std::fs::write(&file, object(n, |key| key)).unwrap();
        let status = std::process::Command::new(env!("CARGO_BIN_EXE_ryii"))
            .args(["query", "--scmpp"])
            .arg(&pattern)
            .args(["--timeout", "1"])
            .arg(&file)
            .output()
            .unwrap()
            .status;
        status.code().unwrap_or(-1) as u64
    });
    eprintln!("{}", growth_table("timeout_latency", &samples));
    assert!(samples.iter().all(|sample| sample.count == 3), "every size exits 3");
    assert_growth_exponent("timeout_latency", &samples, 0.0, 0.3);
}

/// `(x)+` between unanchored siblings splits like `(x)`: C(n, 3) rows, nothing above the output.
#[test]
fn growth_bench_plus_sibling_is_output_bound() {
    let program = "(arguments (identifier) @x (identifier)+ @y (identifier) @z)";
    let call = |n: usize| format!("f({});\n", (0..n).map(|i| format!("a{i}")).collect::<Vec<_>>().join(", "));
    let samples = sample_growth(&[20, 40, 80], RUNS, |n| scmpp_work(program, &call(n)));
    eprintln!("{}", growth_table("plus_sibling", &samples));
    assert_growth_exponent("plus_sibling", &samples, 3.2, 3.6);
}

/// `nth-child N of` ranks the `of` siblings once per statement: linear in the siblings.
#[test]
fn growth_bench_nth_child_of_is_linear() {
    let program = "((pair) @p (#nth-child? @p 3 of (pair)))";
    let samples = sample_growth(&[500, 1000, 2000], RUNS, |n| scmpp_work(program, &object(n, |key| key)));
    eprintln!("{}", growth_table("nth_child_of", &samples));
    assert_growth_exponent("nth_child_of", &samples, 1.2, 1.5);
}

/// The bug report's query shape: 3 siblings on a level that also holds `has-ancestor?`, whose inner
/// `#eq?` reads a sibling's capture. Split: C(n-1, 2) result rows, nothing above the output. Unsplit
/// (the 0.2.3 behavior): tree-sitter's walk over the siblings, timed for the receipt.
#[test]
fn growth_bench_split_with_relation_is_output_bound() {
    let program = r#"((object (pair key: (property_identifier) @k) (pair) @b (pair) @c) (#has-ancestor? @b ((variable_declarator name: (identifier) @name) (#eq? @name @k))))"#;
    let source = |n: usize| object(n, |key| key).replacen("k0: 0", "big: 0", 1);
    let split = sample_growth(&[20, 40, 80], RUNS, |n| scmpp_work(program, &source(n)));
    let whole = sample_growth(&[20, 40, 80], RUNS, |n| scmpp_work_with(hafley_scm::scmpp::compile_unsplit, program, &source(n)));
    eprintln!("{}", growth_table("split_with_relation", &split));
    eprintln!("{}", growth_table("unsplit_with_relation", &whole));
    assert_growth_exponent("split_with_relation", &split, 2.2, 2.6);
    let (_, _, _, unsplit_time) = *hafley_observe::growth_bench::exponents(&whole).last().unwrap();
    assert!(unsplit_time >= 3.0, "the unsplit shape is the combinatorial one: time exponent {unsplit_time:.2}");
}
