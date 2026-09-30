#[path = "../shared.rs"]
mod shared;

fn main() {
    println!("{}", shared::total());
}
