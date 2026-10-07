#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("cleave_rust_outputs", |case| {
        crate::fixture_runner::commands(case, |step| {
            let args = &step["arguments"];
            serde_json::json!(sprefa_extract::move_cx::walk_corpus(
                std::path::Path::new(args["root"].as_str().unwrap()),
                args["tracked"].as_bool().unwrap(),
            )
            .unwrap())
        })
    });
}
