//! Write the debugger's initial cell fixture as a bincode netplay-start snapshot.
use std::io::{self, Write};

fn main() {
    let state = kneeman::terrain_cells::playground();
    let bytes = bincode::serialize(&state).expect("serialize cell fixture");
    io::stdout().lock().write_all(&bytes).expect("write cell fixture");
}
