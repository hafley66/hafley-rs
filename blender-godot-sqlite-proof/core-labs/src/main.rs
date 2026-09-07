use std::{env, fs, path::PathBuf};

fn main() {
    let output = env::args_os().nth(1).map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("results/0_trace.json"));
    if let Some(parent) = output.parent() { fs::create_dir_all(parent).unwrap(); }
    let bytes = serde_json::to_vec_pretty(&core_labs::collision::generate_trace()).unwrap();
    fs::write(&output, bytes).unwrap();
    println!("{}", output.display());
}
