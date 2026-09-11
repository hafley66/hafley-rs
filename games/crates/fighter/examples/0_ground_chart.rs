use std::{env, fs, path::PathBuf};

fn main() {
    let rendered = game_fighter::_4_ground_chart::render();
    if env::args().any(|argument| argument == "--write") {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("5_ground_chart.md");
        fs::write(&path, rendered).expect("write generated ground chart");
        fs::write(
            path.with_extension("d2"),
            game_fighter::_4_ground_chart::render_d2(),
        )
        .expect("write D2 chart");
        println!("wrote {}", path.display());
    } else {
        print!("{rendered}");
    }
}
