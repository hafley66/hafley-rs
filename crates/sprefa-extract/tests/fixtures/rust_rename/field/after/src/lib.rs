mod util;

use crate::util::Helper;

pub struct Other {
    pub size: u32,
}

pub fn width(h: &Helper) -> u32 {
    h.width
}

pub fn other(o: &Other) -> u32 {
    o.size
}

pub fn build() -> u32 {
    let h = Helper { width: 4 };
    let read = h.width;
    let Helper { width: size, .. } = h;
    read + size
}
