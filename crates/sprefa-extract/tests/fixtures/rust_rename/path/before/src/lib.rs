#[path = "elsewhere/impl.rs"]
mod util;
mod other;

use crate::util::Helper;

pub fn build() -> Helper {
    Helper::new()
}
