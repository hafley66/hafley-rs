mod a;
pub mod b;
#[path = "../gen/made.rs"]
mod made;

pub fn root() -> u32 {
    a::inner::value() + made::MADE
}
