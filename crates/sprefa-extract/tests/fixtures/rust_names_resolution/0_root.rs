#[path = "1_shared.rs"]
pub mod shared;
pub use shared::target as renamed;
pub use shared::*;

pub mod nested {
    use crate::shared::target as local;
    pub fn run() { local(); super::shared::target(); }
}

pub fn method_user(item: shared::Item) { item.method(); }
