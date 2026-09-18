mod util;

use crate::util::Helper;

pub struct Other {
    pub size: u32,
}

pub fn width(h: &Helper) -> u32 {
    h.size
}

pub fn other(o: &Other) -> u32 {
    o.size
}

pub fn build() -> u32 {
    let h = Helper { size: 4 };
    let read = h.size;
    let Helper { size, .. } = h;
    read + size
}
