use std::{env, fs, path::PathBuf};

fn main() {
    let rendered = game_fighter::ground_chart::render();
    if env::args().any(|argument| argument == "--write") {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("5_ground_chart.md");
        fs::write(&path, rendered).expect("write generated ground chart");
        println!("wrote {}", path.display());
    } else {
        print!("{rendered}");
    }
}
