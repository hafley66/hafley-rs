#[path = "../../sprefa-extract/src/bin/ryi/gen/models/mod.rs"]
mod models;
#[path = "../../sprefa-extract/src/bin/ryi/gen/ops_auto.rs"]
mod ops_auto;
#[path = "../../sprefa-extract/src/bin/ryi/gen/cli_auto.rs"]
mod cli_auto;
#[path = "../../sprefa-extract/src/bin/ryi/gen/daemon_auto.rs"]
mod daemon_auto;
#[path = "../../sprefa-extract/src/bin/ryi/gen/client_auto.rs"]
mod client_auto;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    client_auto::main().await
}
