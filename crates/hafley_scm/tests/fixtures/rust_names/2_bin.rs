#[path = "1_shared.rs"]
mod included;
fn main() { names_fixture::renamed(); crate::included::target(); }
