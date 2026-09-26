use crate::cli::ServeArgs;

pub fn run(args: ServeArgs) -> Result<(), Box<dyn std::error::Error>> {
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
