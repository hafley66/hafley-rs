use serde_json::{json, Value};

pub fn evaluate(case: &Value) -> Value {
    crate::fixture_runner::commands(case, |step| {
        let result = match step["api"].as_str().unwrap() {
            "indexer_argv" => {
                use sprefa_extract::scip::{GO_SPEC, PYTHON_SPEC, TS_SPEC};
                assert_eq!(GO_SPEC.args.last(), Some(&"./..."));
                assert!(TS_SPEC.args.contains(&"index"));
                assert!(PYTHON_SPEC.args.contains(&"."));
                json!({"go":GO_SPEC.args,"ts":TS_SPEC.args,"python":PYTHON_SPEC.args})
            }
            "last_error_line" => json!(sprefa_extract::scip_ensure::last_error_line(step["text"].as_str().unwrap())),
            "rust_build" => {
                use sprefa_extract::ScipSource;
                let previous = std::env::var_os("PATH");
                std::env::set_var("PATH", step["path"].as_str().unwrap());
                let built = sprefa_extract::ScipRust.build(std::path::Path::new(step["root"].as_str().unwrap()));
                match previous { Some(path) => std::env::set_var("PATH", path), None => std::env::remove_var("PATH") }
                json!(String::from_utf8(std::fs::read(built.unwrap()).unwrap()).unwrap())
            }
            other => panic!("unknown crawl API: {other}"),
        };
        if let Some(expected) = step.get("expect") { assert_eq!(&result, expected, "{step}"); }
        result
    })
}
