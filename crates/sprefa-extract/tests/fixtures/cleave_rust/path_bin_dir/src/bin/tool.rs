#[path = "tool/1_a.rs"]
mod a;

fn two() -> u32 {
    2
}

fn main() {
    println!("{}", a::one() + two());
}
