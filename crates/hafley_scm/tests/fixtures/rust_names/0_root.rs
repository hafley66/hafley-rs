#[path = "1_shared.rs"]
pub mod shared;
pub use shared::target as renamed;
pub use shared::*;
