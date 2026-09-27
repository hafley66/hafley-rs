mod util;
mod maker;

use crate::maker::make;
use crate::util::Helper;

pub fn use_it() -> u32 {
    let v = make();
    v.size
}

pub fn known(h: &Helper) -> u32 {
    h.size
}
