mod util;
mod maker;

use crate::maker::make;

pub fn use_it() -> u32 {
    let v = make();
    v.size
}
