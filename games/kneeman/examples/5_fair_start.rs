//! Shared native-test start for early/late forward-air browser contact acceptance.
use std::io::{self, Write};

fn main() {
    let late = match std::env::args().nth(1).as_deref() {
        Some("--early") => false,
        Some("--late") => true,
        _ => panic!("expected --early or --late"),
    };
    let state = kneeman::fixtures::fair_contact(late);
    io::stdout().lock().write_all(&bincode::serialize(&state).unwrap()).unwrap();
}
