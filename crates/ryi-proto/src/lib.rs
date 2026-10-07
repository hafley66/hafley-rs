// The generated models and request envelope are owned by this crate.
#[rustfmt::skip]
#[path = "gen/cli_auto.rs"]
pub mod cli_auto;
#[rustfmt::skip]
#[path = "gen/daemon_auto.rs"]
pub mod daemon_auto;
#[rustfmt::skip]
#[path = "gen/models/mod.rs"]
pub mod models;
#[rustfmt::skip]
#[path = "gen/ops_auto.rs"]
pub mod ops_auto;

pub const BUILD_DATETIME: &str = env!("SPREFA_BUILD_DATETIME");

#[cfg(test)]
mod tests {
    use super::daemon_auto::Request;

    #[test]
    fn request_keeps_path_spelling_and_carries_the_caller_root() {
        let root = std::path::PathBuf::from("/tmp/ryi-request-root");
        let args = serde_json::json!({"paths": ["src", "-"], "entry": "index.ts"});
        let request = Request::new(root.clone(), &args).unwrap();
        assert_eq!(request.request_root, root);
        assert_eq!(request.args, args);
        let decoded: serde_json::Value = request.decode().unwrap();
        assert_eq!(decoded, args);
    }

    #[test]
    fn root_clap_args_round_trip_through_http_args() {
        use clap::Parser as _;
        let cli = super::cli_auto::Ryi::parse_from(["ryi", "/tmp/unknown.extension"]);
        let args = serde_json::to_value(&cli.file).unwrap();
        let decoded: super::ops_auto::ExtractArgs = serde_json::from_value(args).unwrap();
        assert_eq!(decoded.args.inputs.paths, cli.file.inputs.paths);
    }
}

#[path = "0_build_identity.rs"]
pub mod build_identity;

#[path = "1_help.rs"]
pub mod help;
