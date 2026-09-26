pub use ryi_proto::{daemon_auto, models, ops_auto};

#[path = "gen/cli_auto.rs"]
mod cli_auto;
#[path = "gen/client_auto.rs"]
mod client_auto;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    client_auto::main().await
}
