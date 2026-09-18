#[path = "elsewhere/impl.rs"]
mod util;
mod other;

use crate::util::Tool;

pub fn build() -> Tool {
    Tool::new()
}
