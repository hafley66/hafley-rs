use std::io;
use std::path::PathBuf;

#[path = "1_analyze.rs"]
mod analyze;
#[path = "0_types.rs"]
mod types;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let first = args
        .next()
        .ok_or("usage: lab-dead-files [--rustc] <root> <edges.jsonl> [tool-output]")?;
    if first == "--rustc" {
        let root = args.next().map(PathBuf::from).ok_or("missing root")?;
        let edges_path = args
            .next()
            .map(PathBuf::from)
            .ok_or("missing edges.jsonl")?;
        let cargo_json_path = args
            .next()
            .map(PathBuf::from)
            .ok_or("missing cargo JSONL")?;
        if args.next().is_some() {
            return Err(
                "usage: lab-dead-files --rustc <root> <edges.jsonl> <cargo-check.jsonl>".into(),
            );
        }
        let edges = analyze::read_edges(io::BufReader::new(std::fs::File::open(edges_path)?))?;
        let dead = analyze::dead_files(&root, &edges)?;
        let cargo_json = std::fs::read_to_string(cargo_json_path)?;
        let rustc = analyze::rustc_dead_code_files(&cargo_json, &root)?;
        let comparison = analyze::compare_orphans(&dead, &serde_json::to_string(&rustc)?, &root)?;
        print_comparison(comparison);
        return Ok(());
    }
    let root = PathBuf::from(first);
    let edges_path = args.next().map(PathBuf::from);
    let tool_path = args.next().map(PathBuf::from);
    if args.next().is_some() {
        return Err("usage: lab-dead-files <root> [edges.jsonl] [tool-orphans.txt]".into());
    }
    let input: Box<dyn io::BufRead> = match edges_path {
        Some(path) => Box::new(io::BufReader::new(std::fs::File::open(path)?)),
        None => Box::new(io::stdin().lock()),
    };
    let dead = analyze::dead_files(&root, &analyze::read_edges(input)?)?;
    if let Some(tool_path) = tool_path {
        let tool_output = std::fs::read_to_string(tool_path)?;
        let comparison = analyze::compare_orphans(&dead, &tool_output, &root)?;
        print_comparison(comparison);
    } else {
        for file in dead {
            println!("{}", file.path.display());
        }
    }
    Ok(())
}

fn print_comparison(comparison: analyze::OrphanComparison) {
    println!("agreement\t{}", comparison.both.len());
    println!("ryi_only\t{}", comparison.ryi_only.len());
    println!("tool_only\t{}", comparison.tool_only.len());
    for path in comparison.ryi_only {
        println!("ryi-only\t{path}");
    }
    for path in comparison.tool_only {
        println!("tool-only\t{path}");
    }
}
