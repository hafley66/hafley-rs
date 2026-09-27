extern crate hafley_observe as oh;

oh::counting_allocator!();

use oh::test;
use syn::visit::Visit;
use syn::ItemFn;

fn unstamped_test_functions(path: &str, source: &str) -> Vec<String> {
    struct TestFunctions<'a> {
        path: &'a str,
        unstamped: Vec<String>,
    }

    impl<'ast> Visit<'ast> for TestFunctions<'_> {
        fn visit_item_fn(&mut self, function: &'ast ItemFn) {
            let is_test_name = function.sig.ident.to_string().starts_with("test_");
            let has_test_attribute = function.attrs.iter().any(|attribute| {
                let path = attribute.path();
                path.is_ident("test")
                    || (path.segments.len() == 2
                        && path.segments[0].ident == "oh"
                        && path.segments[1].ident == "test")
            });
            if is_test_name && !has_test_attribute {
                self.unstamped
                    .push(format!("{}:{}", self.path, function.sig.ident));
            }
            syn::visit::visit_item_fn(self, function);
        }
    }

    let mut functions = TestFunctions {
        path,
        unstamped: Vec::new(),
    };
    let parsed = syn::parse_file(source)
        .unwrap_or_else(|error| panic!("could not parse Rust source {path}: {error}"));
    functions.visit_file(&parsed);
    functions.unstamped
}
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
            let relative = path.strip_prefix(root).unwrap().display().to_string();
            let mut findings = unstamped_test_functions(&relative, &source);
            if source.contains("#[test") && !source.contains("use oh::test;") {
                findings.push(relative);
            }
            (!findings.is_empty()).then_some(findings)
        })
        .flatten()
        .collect();
    assert!(
        unstamped.is_empty(),
        "unstamped test functions or test files without `use oh::test;`:\n{}",
        unstamped.join("\n")
    );
}

#[test(time_ms = 1000, logs = 5)]
fn scanner_flags_test_prefixed_functions_without_test_attributes() {
    let source = "fn test_missing_attribute() {}\nfn helper() {}\n#[oh::test] fn test_stamped() {}";
    assert_eq!(
        unstamped_test_functions("fixture.rs", source),
        ["fixture.rs:test_missing_attribute"]
    );
}

#[test(time_ms = 10_000, logs = 8)]
fn sigterm_drain_reexecutes_with_the_same_seed_and_direct_subscriber() {
    if std::env::var_os("OH_REPLAY").is_some() {
        return;
    }
    let ready = std::env::temp_dir().join(format!("oh-sigterm-ready-{}", std::process::id()));
    let drain = std::env::temp_dir().join(format!("oh-sigterm-drain-{}", std::process::id()));
    let replayed = std::env::temp_dir().join(format!("oh-sigterm-replay-{}", std::process::id()));
    let _ = std::fs::remove_file(&ready);
    let _ = std::fs::remove_file(&drain);
    let _ = std::fs::remove_file(&replayed);
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("sigterm_replay_target")
        .arg("--nocapture")
        .env("OH_SEED", "4242")
        .env("OH_READY_PATH", &ready)
        .env("OH_DRAIN_PATH", &drain)
        .env("OH_REPLAY_PATH", &replayed)
        .spawn()
        .expect("spawn signal fixture");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !ready.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(ready.exists(), "child never entered the annotated test");
    let signal_result = unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
    assert_eq!(signal_result, 0, "send SIGTERM to the child");
    let output = child.wait_with_output().expect("wait for replay child");
    let _ = std::fs::remove_file(ready);
    let receipt = std::fs::read_to_string(&drain).expect("drain receipt");
    let replay_receipt = std::fs::read_to_string(&replayed).expect("replay receipt");
    let _ = std::fs::remove_file(drain);
    let _ = std::fs::remove_file(replayed);
    assert!(output.status.success(), "{}", output.status);
    assert!(receipt.contains("drained_at="), "{receipt}");
    assert!(receipt.contains("last_event_at="), "{receipt}");
    assert!(receipt.contains("seed=4242"), "{receipt}");
    assert_eq!(replay_receipt, "seed=4242 direct_subscriber=true\n");
}

#[test(time_ms = 10_000, logs = 8)]
fn sigterm_replay_target() {
    let Some(ready) = std::env::var_os("OH_READY_PATH") else {
        return;
    };
    if std::env::var_os("OH_REPLAY").is_some() {
        assert_eq!(oh::testkit::seed(), 4242);
        std::fs::write(
            std::env::var_os("OH_REPLAY_PATH").unwrap(),
            "seed=4242 direct_subscriber=true\n",
        )
        .unwrap();
        tracing::info!(target: "oh_replay_fixture", "seed={}", oh::testkit::seed());
        return;
    }

    tracing::info!(target: "oh_sigterm_fixture", "before signal");
    std::fs::write(ready, b"ready").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !oh::testkit::termination_requested() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        oh::testkit::termination_requested(),
        "SIGTERM was not observed"
    );
}
