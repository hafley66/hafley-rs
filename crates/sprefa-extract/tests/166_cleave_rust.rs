#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("cleave_rust_outputs", |case| {
        crate::fixture_runner::commands(case, |step| {
            let args = &step["arguments"];
            serde_json::json!(sprefa_extract::move_cx::walk_files_with_untracked(
                std::path::Path::new(args["root"].as_str().unwrap()),
                args["untracked"].as_bool().unwrap(),
            )
            .unwrap())
        })
    });
}
