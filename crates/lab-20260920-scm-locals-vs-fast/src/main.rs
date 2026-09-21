use std::io::IsTerminal;
use std::path::PathBuf;

use lab_20260920_scm_locals_vs_fast::analyze;

fn main() {
    let config = hafley_observe::Config::from_env(
        "lab-20260920-scm-locals-vs-fast",
        env!("CARGO_PKG_VERSION"),
        "lab_20260920_scm_locals_vs_fast=info",
        std::io::stderr().is_terminal(),
    )
    .expect("observability configuration");
    hafley_observe::init(config).expect("observability install");
    let result = run();
    hafley_observe::finish_trace();
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let language = args.next().ok_or("usage: lab <kotlin|ts> <query> <paths...>")?;
    let query_path = PathBuf::from(args.next().ok_or("missing query path")?);
    let paths = args.map(PathBuf::from).collect::<Vec<_>>();
    let query = std::fs::read_to_string(query_path)?;
    let analysis = analyze(&language, &query, &paths)?;
    for edge in analysis.edges {
        println!("{}", serde_json::to_string(&edge)?);
    }
    for unresolved in analysis.unresolved {
        eprintln!("{}", serde_json::to_string(&unresolved)?);
    }
    Ok(())
}
