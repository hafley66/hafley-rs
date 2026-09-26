// The generated models and request envelope are owned by this crate.
#[path = "gen/models/mod.rs"]
pub mod models;
#[path = "gen/ops_auto.rs"]
pub mod ops_auto;
#[path = "gen/daemon_auto.rs"]
pub mod daemon_auto;
#[path = "gen/cli_auto.rs"]
pub mod cli_auto;

pub const BUILD_DATETIME: &str = env!("SPREFA_BUILD_DATETIME");

#[cfg(test)]
mod tests {
    use super::daemon_auto::Request;

    #[test]
    fn client_sends_absolute_paths_and_server_resolves_relative_fallback() {
        let root = std::path::PathBuf::from("/tmp/ryi-request-root");
        let args = serde_json::json!({"paths": ["src", "-"], "entry": "index.ts"});
        let request = Request::new("fast", root.clone(), &args).unwrap();
        assert_eq!(request.args, serde_json::json!({
            "paths": ["/tmp/ryi-request-root/src", "-"],
            "entry": "/tmp/ryi-request-root/index.ts"
        }));
        let relative = Request { request_root: root, args };
        let decoded: serde_json::Value = relative.decode("fast").unwrap();
        assert_eq!(decoded, request.args);
    }
}
