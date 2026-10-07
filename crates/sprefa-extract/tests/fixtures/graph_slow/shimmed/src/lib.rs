mod helper;

pub const SHIM: &str = include_str!("shim.rs");

pub fn build() -> usize {
    helper::assemble()
}
