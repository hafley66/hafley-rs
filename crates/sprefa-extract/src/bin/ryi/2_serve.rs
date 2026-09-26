use clap::Parser;

#[derive(Parser)]
struct ServeArgs {
    #[arg(long, value_name = "HOST:PORT|unix:/PATH")]
    listen: String,
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = ServeArgs::try_parse_from(
        std::iter::once(std::ffi::OsString::from("serve")).chain(std::env::args_os().skip(2)),
    )?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        if let Some(path) = args.listen.strip_prefix("unix:") {
            let listener = tokio::net::UnixListener::bind(path)?;
            axum::serve(listener, crate::http_auto::router()).await?;
        } else {
            let listener = tokio::net::TcpListener::bind(&args.listen).await?;
            axum::serve(listener, crate::http_auto::router()).await?;
        }
        Ok(())
    })
}
