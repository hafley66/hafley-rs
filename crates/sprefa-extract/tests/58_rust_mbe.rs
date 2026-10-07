//! `hafley_scm::lang::rust::expand_file` plus the `RustSource::extract` call-arm
//! hook that splices it in. `RustSource.extract(fixture)` folds the gained
//! facts straight into `call.nodes`/`call.aux.sites`, spans already mapped back
//! to the original file, so these end-to-end counts ARE the lab's "expanded"
//! column (`plans/extract-macro-lab-2026-08-29/PLAN.md` Option 1 table).
//!
//! One expand+extract per mbe fixture lands the whole table in
//! `fixtures/rust_mbe_cases/`; the unexpanded f2/f4/f5/f6/f8, the f1/f3/f7
//! counts, the f9 pass budget, the mkfn macro_site row and both span-mapping
//! claims run as code before the snapshot freezes. SABOTAGE RECEIPT: dropping
//! the `collect_calls` name filter makes f2/f4/f5/f8 fail their `is_none()`
//! check, since `include!`/attribute macros would then be mistaken for local
//! invocations.

#![cfg(feature = "cli")]
use hafley_scm::lang::rust::expand_file;

#[test]
fn whole_output() {
    crate::fixture_runner::run("rust_mbe_cases", crate::rust_mbe_support::evaluate);
}

/// COUNT: `expand_file` is one `ra_ap_syntax` parse per file (plus a second
/// parse per pass that still finds an invocation); ignored by default since it
/// needs a local rust-analyzer checkout. Also the mbe.macro_sites.tsv receipt
/// for `plans/extract-crawl-2026-08-29/rust.REPORT.md` section 14.
#[test]
#[ignore]
fn corpus_wall_time_and_macro_sites_tsv() {
    let corpus = std::path::Path::new("/Users/chrishafley/projects/rust-analyzer/crates");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(corpus).expect("rust-analyzer checkout at this path") {
        let crate_dir = entry.unwrap().path().join("src");
        rs_files_under(&crate_dir, &mut files);
    }
    files.sort();

    let t0 = std::time::Instant::now();
    let mut tsv = String::from("path\tstart\tend\tmacro_name\n");
    let mut files_with_macros = 0usize;
    let mut budget_hits = 0usize;
    for path in &files {
        let content = std::fs::read_to_string(path).unwrap();
        let Some(expanded) = expand_file(&content) else {
            continue;
        };
        files_with_macros += 1;
        if expanded.budget_hit {
            budget_hits += 1;
            eprintln!("budget_hit: {}", path.display());
        }
        let rel = path
            .strip_prefix("/Users/chrishafley/projects/rust-analyzer/")
            .unwrap();
        for (span, name) in expanded.macro_sites() {
            tsv.push_str(&format!(
                "{}\t{}\t{}\t{}\n",
                rel.display(),
                span.start,
                span.end,
                name
            ));
        }
    }
    let wall = t0.elapsed();

    std::fs::write(
        "../../plans/extract-macro-lab-2026-08-29/mbe.macro_sites.tsv",
        &tsv,
    )
    .expect("plans dir is writable from the crate root");

    eprintln!(
        "files={} with_macros={} budget_hits={} wall_ms={}",
        files.len(),
        files_with_macros,
        budget_hits,
        wall.as_millis()
    );
    crate::wall_bench::check(
        "tests/58_rust_mbe.rs:corpus_wall_time_and_macro_sites_tsv",
        wall.as_secs() as f64,
        2.0,
        false,
    );
}

fn rs_files_under(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rs_files_under(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}
