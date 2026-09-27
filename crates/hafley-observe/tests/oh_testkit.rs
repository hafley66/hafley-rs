extern crate hafley_observe as oh;

oh::counting_allocator!();

use oh::test;
use std::panic::{catch_unwind, AssertUnwindSafe};

#[test(time_ms = 1000, logs = 4)]
fn imported_test_attribute_emits_a_builtin_test() {
    tracing::info!(target: "oh_testkit_fixture", "one event");
}

#[oh::budget(time_ms = 1000, logs = 1)]
fn budget_attribute_wraps_a_function() {
    tracing::info!(target: "oh_budget_fixture", "one event");
}

#[oh::test(time_ms = 1000, logs = 2)]
fn budget_attribute_runs_inside_test_attribute() {
    budget_attribute_wraps_a_function();
}

#[oh::instrument_all]
mod instrumented {
    pub fn included() {
        tracing::info!(target: "oh_instrument_fixture", "included");
    }

    #[oh::skip]
    pub fn skipped() {
        tracing::info!(target: "oh_instrument_fixture", "skipped");
    }

    #[derive(Debug)]
    pub struct Worker;

    impl Worker {
        pub fn run(&self) {
            tracing::info!(target: "oh_instrument_fixture", "method");
        }

        #[oh::skip]
        pub fn skip(&self) {
            tracing::info!(target: "oh_instrument_fixture", "skipped method");
        }
    }
}

#[oh::test(time_ms = 1000, logs = 4)]
fn instrument_all_accepts_inline_modules_and_impls() {
    instrumented::included();
    instrumented::skipped();
    instrumented::Worker.run();
    instrumented::Worker.skip();
}

fn repeated_callsite() {
    tracing::info!(target: "oh_over_budget_fixture", "repeat");
}

#[oh::test(time_ms = 1000, logs = 5)]
fn log_budget_counts_repeated_events_at_one_callsite() {
    let result = catch_unwind(AssertUnwindSafe(|| {
        oh::testkit::run(
            "log_budget_fixture",
            oh::testkit::Budget::new(None, Some(1), None),
            || {
                repeated_callsite();
                repeated_callsite();
            },
        )
    }));
    assert!(result.is_err());
}

#[oh::test(time_ms = 1000, logs = 5)]
fn memory_budget_uses_live_and_peak_allocator_counters() {
    let result = catch_unwind(AssertUnwindSafe(|| {
        oh::testkit::run(
            "memory_budget_fixture",
            oh::testkit::Budget::new(None, None, Some(8)),
            || vec![0_u8; 1024],
        )
    }));
    assert!(result.is_err());
}

#[oh::test(time_ms = 1000, logs = 5)]
fn time_budget_reports_elapsed_limit() {
    let result = catch_unwind(AssertUnwindSafe(|| {
        oh::testkit::run(
            "time_budget_fixture",
            oh::testkit::Budget::new(Some(1), None, None),
            || std::thread::sleep(std::time::Duration::from_millis(5)),
        )
    }));
    assert!(result.is_err());
}

#[test(time_ms = 1000, logs = 5)]
fn every_test_file_imports_oh_test() {
    fn rust_files(root: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(root).expect("test source directory") {
            let path = entry.expect("directory entry").path();
            if path.is_dir() {
                rust_files(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    rust_files(&root.join("tests"), &mut files);

    let unstamped: Vec<_> = files
        .into_iter()
        .filter_map(|path| {
            let source = std::fs::read_to_string(&path).expect("Rust source");
            (source.contains("#[test") && !source.contains("use oh::test;"))
                .then(|| path.strip_prefix(root).unwrap().display().to_string())
        })
        .collect();
    assert!(
        unstamped.is_empty(),
        "test attributes without `use oh::test;`:\n{}",
        unstamped.join("\n")
    );
}
