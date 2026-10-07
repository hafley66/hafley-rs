pub use ryi_proto::{cli_auto, daemon_auto, models, ops_auto};
#[rustfmt::skip]
#[path = "gen/client_auto.rs"]
mod client_auto;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    if ryi_proto::build_identity::startup("ryi") || ryi_proto::help::local_help() {
        return std::process::ExitCode::SUCCESS;
    }
    client_auto::main().await
}
